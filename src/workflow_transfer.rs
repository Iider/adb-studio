use crate::{
    App, canvas,
    drafts::{Draft, DraftNode},
    types::Workflow,
    vision,
};
use anyhow::{Context, ensure};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

mod import;
pub use import::cleanup_imports;

const FILE_LIMIT: usize = 32 * 1024 * 1024;
const CONTENT_LIMIT: usize = 2 * 1024 * 1024 - 1024;
const ASSET_LIMIT: usize = 20 * 1024 * 1024;
const ASSET_COUNT_LIMIT: usize = 40;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortableDraft {
    id: String,
    name: String,
    description: String,
    entry: String,
    nodes: Vec<DraftNode>,
    viewport: [f64; 3],
}

impl From<&Draft> for PortableDraft {
    fn from(draft: &Draft) -> Self {
        Self {
            id: draft.id.clone(),
            name: draft.name.clone(),
            description: draft.description.clone(),
            entry: draft.entry.clone(),
            nodes: draft.nodes.clone(),
            viewport: draft.viewport,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    workflow: Option<Workflow>,
    draft: Option<PortableDraft>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    id: String,
    mime: String,
    base64: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowFile {
    format: String,
    format_revision: u32,
    content: Content,
    assets: Vec<Asset>,
}

type Failure = (StatusCode, Json<Value>);
type ExportResult = Result<(HeaderMap, Json<WorkflowFile>), Failure>;

fn failure(status: StatusCode, message: impl ToString) -> Failure {
    (status, Json(json!({"error": message.to_string()})))
}

fn storage_error(error: impl std::fmt::Display) -> Failure {
    tracing::error!("读取工作流导出内容失败：{error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "导出失败，请稍后重试")
}

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .merge(import::router())
        .route("/workflows/{id}/export", get(export_workflow))
        .route("/workflow-drafts/{id}/export", get(export_draft))
}

fn normalized_workflow(workflow: Workflow) -> anyhow::Result<Value> {
    let mut workflow = workflow;
    workflow.devices.sort();
    Ok(serde_json::to_value(workflow)?)
}

fn draft_state(workflow: &Workflow, draft: Option<&Draft>) -> &'static str {
    let Some(draft) = draft else { return "none" };
    match canvas::compile(draft) {
        Ok(compiled) => {
            if normalized_workflow(compiled).ok() == normalized_workflow(workflow.clone()).ok() {
                "saved"
            } else {
                "modified"
            }
        }
        Err(_) => "incomplete",
    }
}

fn check_content_size(content: &Content) -> anyhow::Result<()> {
    if let Some(workflow) = &content.workflow {
        ensure!(
            serde_json::to_vec(workflow)?.len() <= CONTENT_LIMIT,
            "任务内容过大，超过后续编辑保存的大小限制；当前内容仍保留"
        );
    }
    if let Some(draft) = &content.draft {
        ensure!(
            serde_json::to_vec(draft)?.len() <= CONTENT_LIMIT,
            "草稿内容过大，超过后续自动保存的大小限制；当前编辑仍保留"
        );
    }
    Ok(())
}

fn sample_ids(content: &Content) -> anyhow::Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    if let Some(workflow) = &content.workflow {
        for sample in &workflow.tests {
            if let Some(image) = &sample.image {
                let uuid = image.strip_suffix(".png").context("样本图片名称无效")?;
                ensure!(
                    uuid::Uuid::parse_str(uuid).is_ok(),
                    "样本图片名称无效：{image}"
                );
                ids.insert(image.clone());
            }
        }
    }
    ensure!(ids.len() <= ASSET_COUNT_LIMIT, "图片样本最多携带 40 张");
    Ok(ids)
}

async fn build_file(app: &App, content: Content) -> Result<WorkflowFile, Failure> {
    check_content_size(&content).map_err(|error| failure(StatusCode::PAYLOAD_TOO_LARGE, error))?;
    let ids = sample_ids(&content).map_err(|error| failure(StatusCode::BAD_REQUEST, error))?;
    let mut assets = Vec::new();
    let mut total = 0usize;
    for id in ids {
        let path = app.data.join("vision-samples").join(&id);
        let metadata = tokio::fs::metadata(&path).await.map_err(|_| {
            failure(
                StatusCode::BAD_REQUEST,
                format!("样本图片不存在或无法读取：{id}"),
            )
        })?;
        if metadata.len() > 8 * 1024 * 1024 || total as u64 + metadata.len() > ASSET_LIMIT as u64 {
            return Err(failure(
                StatusCode::PAYLOAD_TOO_LARGE,
                "图片样本超过大小限制：单张 8 MiB，合计 20 MiB",
            ));
        }
        let png = tokio::fs::read(&path).await.map_err(storage_error)?;
        total += png.len();
        if total > ASSET_LIMIT {
            return Err(failure(
                StatusCode::PAYLOAD_TOO_LARGE,
                "图片样本合计不能超过 20 MiB",
            ));
        }
        let encoded = tokio::task::spawn_blocking(move || {
            let frame = vision::decode(png)?;
            Ok::<_, anyhow::Error>(STANDARD.encode(frame.png))
        })
        .await
        .map_err(storage_error)?
        .map_err(|error| failure(StatusCode::BAD_REQUEST, format!("样本图片 {id}：{error}")))?;
        assets.push(Asset {
            id,
            mime: "image/png".into(),
            base64: encoded,
        });
    }
    let file = WorkflowFile {
        format: "adb-studio.workflow".into(),
        format_revision: 1,
        content,
        assets,
    };
    if serde_json::to_vec(&file).map_err(storage_error)?.len() > FILE_LIMIT {
        return Err(failure(
            StatusCode::PAYLOAD_TOO_LARGE,
            "导出文件不能超过 32 MiB",
        ));
    }
    Ok(file)
}

fn download_headers(state: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=workflow.workflow.json"
            .parse()
            .unwrap(),
    );
    headers.insert("x-workflow-draft-state", state.parse().unwrap());
    headers
}

async fn export_workflow(State(app): State<Arc<App>>, Path(id): Path<String>) -> ExportResult {
    let (workflow, draft) = app
        .db
        .workflow_export_source(&id)
        .await
        .map_err(storage_error)?
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "任务不存在"))?;
    let state = draft_state(&workflow, draft.as_ref());
    let content = Content {
        workflow: Some(workflow),
        draft: draft.as_ref().map(PortableDraft::from),
    };
    Ok((
        download_headers(state),
        Json(build_file(&app, content).await?),
    ))
}

#[derive(Deserialize)]
pub struct DraftQuery {
    revision: u32,
}

async fn export_draft(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<DraftQuery>,
) -> ExportResult {
    let draft = app
        .db
        .get::<Draft>("workflow_drafts", &id)
        .await
        .map_err(storage_error)?
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "草稿不存在"))?;
    if query.revision != draft.revision {
        return Err(failure(
            StatusCode::CONFLICT,
            "草稿已被其他页面修改，请重新读取或保存副本后再导出",
        ));
    }
    let content = Content {
        workflow: None,
        draft: Some(PortableDraft::from(&draft)),
    };
    Ok((
        download_headers("draft"),
        Json(build_file(&app, content).await?),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        serde_json::from_value(json!({
            "id":uuid::Uuid::new_v4().to_string(), "revision":4, "updated_at":"private timestamp",
            "name":"等待", "description":"", "entry":"wait", "viewport":[5,8,0.8],
            "nodes":[
                {"id":"wait","kind":"wait","label":"等待","config":{"seconds":1},"targets":{"next":"end"},"position":[10,20]},
                {"id":"end","kind":"end","label":"结束","config":{"result":"succeeded"},"targets":{},"position":[30,40]}
            ]
        })).unwrap()
    }

    #[test]
    fn keeps_layout_without_server_fields_and_distinguishes_unsaved_changes() {
        let mut draft = draft();
        let workflow = canvas::compile(&draft).unwrap();
        assert_eq!(draft_state(&workflow, Some(&draft)), "saved");
        draft.nodes[0].position = [60., 90.];
        assert_eq!(draft_state(&workflow, Some(&draft)), "saved");
        let portable = serde_json::to_value(PortableDraft::from(&draft)).unwrap();
        assert!(portable.get("revision").is_none());
        assert!(portable.get("updated_at").is_none());
        assert_eq!(portable["nodes"][0]["position"], json!([60., 90.]));
        draft.nodes[0].config["seconds"] = json!(3);
        assert_eq!(draft_state(&workflow, Some(&draft)), "modified");
        draft.nodes[0].targets.clear();
        assert_eq!(draft_state(&workflow, Some(&draft)), "incomplete");
    }

    #[test]
    fn deduplicates_samples_and_rejects_paths_and_oversized_content() {
        let mut workflow = canvas::compile(&draft()).unwrap();
        let image = format!("{}.png", uuid::Uuid::new_v4());
        for sample in &mut workflow.tests {
            sample.image = Some(image.clone());
        }
        let mut content = Content {
            workflow: Some(workflow),
            draft: None,
        };
        assert_eq!(sample_ids(&content).unwrap().len(), 1);
        content.workflow.as_mut().unwrap().tests[0].image = Some("../private.png".into());
        assert!(sample_ids(&content).is_err());
        content.workflow.as_mut().unwrap().description = "x".repeat(CONTENT_LIMIT);
        assert!(check_content_size(&content).is_err());
    }
}
