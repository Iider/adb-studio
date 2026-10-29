use super::*;
use crate::{
    drafts, engine,
    types::{Device, now},
};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, rejection::BytesRejection},
    routing::post,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation_id: String,
    file: Value,
    #[serde(default)]
    name: Option<String>,
}

struct Validated {
    operation: String,
    digest: String,
    content: Content,
    images: Vec<(String, Vec<u8>)>,
}

pub(super) fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/workflow-import/preview", post(preview))
        .route("/workflow-import", post(save))
        .layer(DefaultBodyLimit::max(FILE_LIMIT))
}

fn internal(error: impl std::fmt::Display) -> Failure {
    tracing::error!("导入工作流失败：{error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "导入失败，请重试")
}

fn make_draft(portable: &PortableDraft) -> anyhow::Result<Draft> {
    let mut value = serde_json::to_value(portable)?;
    value["revision"] = json!(1);
    value["updated_at"] = json!(now());
    Ok(serde_json::from_value(value)?)
}

fn validate(bytes: &[u8]) -> anyhow::Result<Validated> {
    ensure!(bytes.len() <= FILE_LIMIT, "文件不能超过 32 MiB");
    let request: Request =
        serde_json::from_slice(bytes).context("无法读取工作流文件，请选择有效的 JSON 文件")?;
    ensure!(
        uuid::Uuid::parse_str(&request.operation_id).is_ok(),
        "导入操作编号无效"
    );
    let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&request)?));
    let mut file: WorkflowFile = serde_json::from_value(request.file)
        .context("请使用工作流编辑器导出的文件；不支持旧任务文件")?;
    ensure!(
        file.format == "adb-studio.workflow" && matches!(file.format_revision, 1 | 2),
        "不支持此工作流文件格式"
    );
    ensure!(
        file.content.workflow.is_some() || file.content.draft.is_some(),
        "文件没有工作流或草稿"
    );
    ensure!(
        file.content.draft.is_some(),
        "文件缺少画布草稿；不支持旧任务文件"
    );
    ensure!(
        file.format_revision == 2 || !has_inputs(&file.content),
        "含变量的文件需使用第 2 版格式"
    );
    if let Some(workflow) = file.content.workflow.as_mut() {
        engine::validate(workflow)?;
    }
    if let Some(draft) = &file.content.draft {
        ensure!(uuid::Uuid::parse_str(&draft.id).is_ok(), "草稿编号无效");
        drafts::validate(&make_draft(draft)?)?;
        if let Some(workflow) = &file.content.workflow {
            ensure!(
                workflow.id == format!("manual-{}", draft.id),
                "工作流与草稿不属于同一任务"
            );
        }
    }
    if let Some(name) = request.name {
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.len() <= 240,
            "名称不能为空且最多 240 字节"
        );
        if let Some(workflow) = file.content.workflow.as_mut() {
            if let Some(draft) = file.content.draft.as_mut() {
                if draft.name == workflow.name {
                    draft.name = name.into();
                }
            }
            workflow.name = name.into();
        } else if let Some(draft) = file.content.draft.as_mut() {
            draft.name = name.into();
        }
    }
    if let Some(workflow) = &file.content.workflow {
        engine::validate(workflow)?;
    }
    if let Some(draft) = &file.content.draft {
        drafts::validate(&make_draft(draft)?)?;
    }
    check_content_size(&file.content)?;
    let wanted = sample_ids(&file.content)?;
    ensure!(file.assets.len() <= ASSET_COUNT_LIMIT, "图片样本最多 40 张");
    let mut seen = BTreeSet::new();
    let mut images = Vec::new();
    let mut total = 0;
    for asset in file.assets {
        ensure!(
            wanted.contains(&asset.id) && seen.insert(asset.id.clone()),
            "存在重复或多余的图片附件：{}",
            asset.id
        );
        ensure!(asset.mime == "image/png", "只支持 PNG 图片附件");
        ensure!(
            asset.base64.len() <= (8 * 1024 * 1024 + 2) / 3 * 4,
            "单张图片不能超过 8 MiB"
        );
        let png = STANDARD
            .decode(asset.base64)
            .context("图片 Base64 内容无效")?;
        total += png.len();
        ensure!(total <= ASSET_LIMIT, "图片样本合计不能超过 20 MiB");
        let frame = vision::decode(png).with_context(|| format!("样本图片 {} 无效", asset.id))?;
        images.push((asset.id, frame.png));
    }
    ensure!(wanted == seen, "文件缺少工作流引用的图片附件");
    Ok(Validated {
        operation: request.operation_id,
        digest,
        content: file.content,
        images,
    })
}

async fn parse(app: &App, bytes: Result<Bytes, BytesRejection>) -> Result<Validated, Failure> {
    let bytes = bytes
        .map_err(|error| failure(error.status(), "无法读取导入文件，请确认请求不超过 32 MiB"))?;
    let _permit = app.vision_checks.acquire().await.map_err(internal)?;
    tokio::task::spawn_blocking(move || validate(&bytes))
        .await
        .map_err(internal)?
        .map_err(|error| failure(StatusCode::BAD_REQUEST, error))
}

fn name(content: &Content) -> &str {
    content
        .workflow
        .as_ref()
        .map(|w| w.name.as_str())
        .or_else(|| content.draft.as_ref().map(|d| d.name.as_str()))
        .unwrap_or("")
}

fn device_ids(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (key, item) in fields {
                if key == "device_id" {
                    if let Some(id) = item.as_str() {
                        ids.insert(id.into());
                    }
                }
                if key == "devices" {
                    if let Some(values) = item.as_array() {
                        for value in values {
                            if let Some(id) = value.as_str() {
                                ids.insert(id.into());
                            }
                        }
                    }
                }
                device_ids(item, ids);
            }
        }
        Value::Array(items) => {
            for item in items {
                device_ids(item, ids);
            }
        }
        _ => {}
    }
}

async fn preview(
    State(app): State<Arc<App>>,
    bytes: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, Failure> {
    let parsed = parse(&app, bytes).await?;
    let workflows = app
        .db
        .list::<Workflow>("workflows")
        .await
        .map_err(internal)?;
    let drafts = app
        .db
        .list::<Draft>("workflow_drafts")
        .await
        .map_err(internal)?;
    let original = name(&parsed.content);
    let mut suggested = original.to_owned();
    if workflows.iter().any(|w| w.name == original) || drafts.iter().any(|d| d.name == original) {
        let renames_draft = parsed
            .content
            .draft
            .as_ref()
            .is_some_and(|d| d.name == original);
        while suggested.len() + "（导入）".len() > 240
            || (renames_draft && suggested.chars().count() + 4 > 80)
        {
            suggested.pop();
        }
        suggested.push_str("（导入）");
    }
    let devices = app.db.list::<Device>("devices").await.map_err(internal)?;
    let mut ids = BTreeSet::new();
    device_ids(
        &serde_json::to_value(&parsed.content).map_err(internal)?,
        &mut ids,
    );
    let mut warnings = Vec::new();
    let phones: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let device = devices.iter().find(|d| d.id == id);
            let status = match device {
                Some(d) if d.online => "在线",
                Some(_) => "未连接",
                None => "未登记",
            };
            if status != "在线" {
                warnings.push(format!(
                    "原手机{status}：{id}。保留原标注，使用其他手机前需重新标注。"
                ));
            }
            json!({"id":id,"name":device.map(|d|d.name.as_str()).unwrap_or(&id),"status":status})
        })
        .collect();
    if let (Some(workflow), Some(draft)) = (&parsed.content.workflow, &parsed.content.draft) {
        if draft_state(workflow, Some(&make_draft(draft).map_err(internal)?)) != "saved" {
            warnings.push("文件同时包含已保存任务和待保存编辑；导入后分别保留。".into());
        }
    }
    Ok(Json(
        json!({"name":suggested,"kind":match (&parsed.content.workflow,&parsed.content.draft) {(Some(_),Some(_))=>"工作流与画布草稿",(Some(_),None)=>"工作流",_=>"画布草稿"},
        "steps":parsed.content.draft.as_ref().map(|d|d.nodes.len()).or_else(||parsed.content.workflow.as_ref().map(|w|w.nodes.len())),
        "images":parsed.images.len(),"devices":phones,"warnings":warnings}),
    ))
}

async fn replay(app: &App, parsed: &Validated) -> Result<Option<Value>, Failure> {
    let Some(record) = app
        .db
        .get::<Value>("workflow_imports", &parsed.operation)
        .await
        .map_err(internal)?
    else {
        return Ok(None);
    };
    if record["digest"] != parsed.digest {
        return Err(failure(
            StatusCode::CONFLICT,
            "此导入操作已用于其他内容，请重新选择文件",
        ));
    }
    let mut result = record["result"].clone();
    let (kind, id) = if let Some(id) = result["workflow_id"].as_str() {
        ("workflows", id)
    } else {
        ("workflow_drafts", result["draft_id"].as_str().unwrap_or(""))
    };
    if app
        .db
        .get::<Value>(kind, id)
        .await
        .map_err(internal)?
        .is_none()
    {
        result["removed"] = json!(true);
    }
    Ok(Some(result))
}

async fn save(
    State(app): State<Arc<App>>,
    bytes: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, Failure> {
    let mut parsed = parse(&app, bytes).await?;
    let _gate = app.run_gate.lock().await;
    if let Some(result) = replay(&app, &parsed).await? {
        return Ok(Json(result));
    }
    let draft_id = uuid::Uuid::new_v4().to_string();
    let workflow_id = format!("manual-{draft_id}");
    if let Some(workflow) = parsed.content.workflow.as_mut() {
        workflow.id = workflow_id.clone();
    }
    if let Some(draft) = parsed.content.draft.as_mut() {
        draft.id = draft_id.clone();
    }
    let replacements: BTreeMap<_, _> = parsed
        .images
        .iter()
        .map(|(old, _)| (old.clone(), format!("{}.png", uuid::Uuid::new_v4())))
        .collect();
    if let Some(workflow) = parsed.content.workflow.as_mut() {
        for test in &mut workflow.tests {
            if let Some(image) = &mut test.image {
                *image = replacements[image].clone();
            }
        }
    }
    let draft = parsed
        .content
        .draft
        .as_ref()
        .map(make_draft)
        .transpose()
        .map_err(internal)?;
    let result = json!({"workflow_id":parsed.content.workflow.as_ref().map(|w|&w.id),"draft_id":draft.as_ref().map(|d|&d.id),"name":name(&parsed.content),"removed":false});
    let staging = app
        .data
        .join("import-staging")
        .join(uuid::Uuid::new_v4().to_string());
    let samples = app.data.join("vision-samples");
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(internal)?;
    let files: Vec<_> = replacements.values().cloned().collect();
    // 清单先落盘；进程中断后，只清理本次新增且未被工作流引用的图片。
    tokio::fs::write(
        staging.join("manifest.json"),
        serde_json::to_vec(&files).map_err(internal)?,
    )
    .await
    .map_err(internal)?;
    let mut outcome = async {
        tokio::fs::create_dir_all(&samples).await?;
        for (old, png) in &parsed.images {
            let new = &replacements[old];
            tokio::fs::write(staging.join(new), png).await?;
            tokio::fs::rename(staging.join(new), samples.join(new)).await?;
        }
        app.db
            .insert_workflow_import(
                &parsed.operation,
                &parsed.digest,
                &result,
                parsed.content.workflow.as_ref(),
                draft.as_ref(),
            )
            .await
    }
    .await;
    let mut keep_staging = false;
    if outcome.is_err() {
        // 提交结果不明时先查操作记录，避免移除已经提交的样本。
        match app
            .db
            .get::<Value>("workflow_imports", &parsed.operation)
            .await
        {
            Ok(Some(record)) if record["digest"] == parsed.digest => outcome = Ok(()),
            Ok(None) => {
                for file in &files {
                    match tokio::fs::remove_file(samples.join(file)).await {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(_) => keep_staging = true,
                    }
                }
            }
            _ => keep_staging = true,
        }
    }
    if !keep_staging {
        if let Err(error) = tokio::fs::remove_dir_all(&staging).await {
            tracing::warn!("导入临时目录等待清理：{error}");
        }
    }
    outcome.map_err(|error| {
        tracing::error!("导入工作流写入失败：{error}");
        failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "导入保存失败，请重试；已有任务未改变",
        )
    })?;
    let _ = app.events.send(json!({"type":"changed"}));
    Ok(Json(result))
}

pub async fn cleanup_imports(app: &App) -> anyhow::Result<()> {
    let folder = app.data.join("import-staging");
    if !folder.exists() {
        return Ok(());
    }
    let mut referenced = HashSet::new();
    for workflow in app.db.list::<Workflow>("workflows").await? {
        for test in workflow.tests {
            if let Some(image) = test.image {
                referenced.insert(image);
            }
        }
    }
    let mut entries = tokio::fs::read_dir(&folder).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir()
            || uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err()
        {
            continue;
        }
        let manifest = entry.path().join("manifest.json");
        if let Ok(bytes) = tokio::fs::read(manifest).await {
            if let Ok(files) = serde_json::from_slice::<Vec<String>>(&bytes) {
                for file in files {
                    if file
                        .strip_suffix(".png")
                        .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
                        && !referenced.contains(&file)
                    {
                        match tokio::fs::remove_file(app.data.join("vision-samples").join(file))
                            .await
                        {
                            Ok(()) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => return Err(e.into()),
                        }
                    }
                }
            }
        }
        tokio::fs::remove_dir_all(entry.path()).await?;
    }
    Ok(())
}
