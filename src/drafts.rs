use crate::{App, types::now};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub config: Value,
    pub targets: HashMap<String, String>,
    pub position: [f64; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<crate::workflow_inputs::Input>,
    pub id: String,
    pub revision: u32,
    pub name: String,
    pub description: String,
    pub entry: String,
    pub nodes: Vec<DraftNode>,
    pub viewport: [f64; 3],
    #[serde(default)]
    updated_at: String,
}

type Failure = (StatusCode, Json<Value>);
type Result<T> = std::result::Result<T, Failure>;

fn failure(status: StatusCode, message: impl ToString) -> Failure {
    (status, Json(json!({"error":message.to_string()})))
}
fn internal(error: impl ToString) -> Failure {
    tracing::error!("草稿存储失败：{}", error.to_string());
    failure(
        StatusCode::INTERNAL_SERVER_ERROR,
        "草稿保存失败，请稍后重试",
    )
}

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/workflow-drafts", get(list))
        .route("/workflow-drafts/{id}", put(save).get(read).delete(remove))
}

async fn list(State(app): State<Arc<App>>) -> Result<Json<Vec<Draft>>> {
    let mut drafts = app
        .db
        .list::<Draft>("workflow_drafts")
        .await
        .map_err(internal)?;
    drafts.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(Json(drafts))
}

async fn read(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Draft>> {
    app.db
        .get("workflow_drafts", &id)
        .await
        .map_err(internal)?
        .map(Json)
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "草稿不存在"))
}

async fn save(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(mut draft): Json<Draft>,
) -> Result<Json<Draft>> {
    if id != draft.id || uuid::Uuid::parse_str(&id).is_err() {
        return Err(failure(StatusCode::BAD_REQUEST, "草稿编号无效"));
    }
    validate(&draft).map_err(|error| failure(StatusCode::BAD_REQUEST, error))?;
    // 复用现有短事务锁，使版本检查与写入之间不会插入另一个保存请求。
    let _gate = app.run_gate.lock().await;
    let current = app
        .db
        .get::<Draft>("workflow_drafts", &id)
        .await
        .map_err(internal)?;
    if current.as_ref().map_or(0, |item| item.revision) != draft.revision {
        return Err(failure(
            StatusCode::CONFLICT,
            "另一个页面已修改或删除这份草稿，请重新读取，或将当前内容保存为副本",
        ));
    }
    draft.revision = draft
        .revision
        .checked_add(1)
        .ok_or_else(|| failure(StatusCode::BAD_REQUEST, "草稿保存次数超出范围"))?;
    draft.updated_at = now();
    app.db
        .put("workflow_drafts", &id, &draft)
        .await
        .map_err(internal)?;
    Ok(Json(draft))
}

#[derive(Deserialize)]
struct Revision {
    revision: u32,
}

async fn remove(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<Revision>,
) -> Result<Json<Value>> {
    let _gate = app.run_gate.lock().await;
    let current = app
        .db
        .get::<Draft>("workflow_drafts", &id)
        .await
        .map_err(internal)?
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "草稿不存在"))?;
    if current.revision != query.revision {
        return Err(failure(
            StatusCode::CONFLICT,
            "草稿已在其他页面修改，请重新读取后再删除",
        ));
    }
    if app
        .db
        .get::<Value>("workflows", &format!("manual-{id}"))
        .await
        .map_err(internal)?
        .is_some()
    {
        return Err(failure(
            StatusCode::CONFLICT,
            "这份草稿关联已保存任务，请到任务列表删除整个任务",
        ));
    }
    app.db
        .delete("workflow_drafts", &id)
        .await
        .map_err(internal)?;
    Ok(Json(json!({"ok":true})))
}

pub fn validate(draft: &Draft) -> anyhow::Result<()> {
    crate::workflow_inputs::validate_definitions(&draft.inputs, false)?;
    anyhow::ensure!(
        !draft.name.trim().is_empty() && draft.name.chars().count() <= 80,
        "草稿名称需为 1 至 80 字"
    );
    anyhow::ensure!(
        draft.description.chars().count() <= 2000,
        "草稿说明最多 2000 字"
    );
    anyhow::ensure!(draft.nodes.len() <= 200, "最多添加 200 个步骤");
    anyhow::ensure!(
        draft.viewport.iter().all(|n| n.is_finite())
            && draft.viewport[..2].iter().all(|n| n.abs() <= 1_000_000.)
            && (0.15..=3.).contains(&draft.viewport[2]),
        "画布视角无效"
    );
    let ids: HashSet<_> = draft.nodes.iter().map(|node| node.id.as_str()).collect();
    anyhow::ensure!(ids.len() == draft.nodes.len(), "步骤编号重复");
    anyhow::ensure!(
        draft.entry.is_empty() || ids.contains(draft.entry.as_str()),
        "入口步骤不存在"
    );
    let active = connected_ids(draft);
    for node in &draft.nodes {
        anyhow::ensure!(
            !node.id.is_empty()
                && node.id != "__entry"
                && node.id.len() <= 80
                && node
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c)),
            "步骤编号无效"
        );
        let ports: &[&str] = match node.kind.as_str() {
            "back" | "home" | "recents" | "wait" | "tap" | "swipe" => &["next"],
            "loop" => &["body", "done"],
            "condition" => &["yes", "no"],
            "wait_condition" => &["yes", "timeout"],
            "break" | "continue" | "end" => &[],
            _ => anyhow::bail!("步骤类型不支持"),
        };
        anyhow::ensure!(
            !node.label.trim().is_empty() && node.label.chars().count() <= 120,
            "步骤名称需为 1 至 120 字"
        );
        anyhow::ensure!(
            node.position
                .iter()
                .all(|n| n.is_finite() && n.abs() <= 1_000_000.),
            "步骤位置无效"
        );
        anyhow::ensure!(
            node.config.is_object() && serde_json::to_vec(&node.config)?.len() <= 65536,
            "步骤设置无效或过大"
        );
        if active.contains(node.id.as_str()) {
            validate_config(&node.config, &node.kind)?;
        }
        anyhow::ensure!(
            node.targets
                .iter()
                .all(|(port, target)| ports.contains(&port.as_str())
                    && ids.contains(target.as_str())),
            "步骤连线无效"
        );
    }
    Ok(())
}

// 只有完全没有连线且不是入口的节点可以作为备用节点忽略。
pub fn connected_ids(draft: &Draft) -> HashSet<&str> {
    let mut ids = HashSet::from([draft.entry.as_str()]);
    for node in &draft.nodes {
        if !node.targets.is_empty() {
            ids.insert(node.id.as_str());
            ids.extend(node.targets.values().map(String::as_str));
        }
    }
    ids
}

fn validate_config(config: &Value, kind: &str) -> anyhow::Result<()> {
    for (key, value) in config.as_object().expect("已检查为对象") {
        let valid = match key.as_str() {
            "seconds" | "count" | "interval" | "after_wait" | "x" | "y" | "from_x" | "from_y"
            | "to_x" | "to_y" | "duration_ms" | "timeout" => {
                value.is_number()
                    || (crate::workflow_inputs::field_allowed(kind, key)
                        && (crate::workflow_inputs::reference(value).is_some()
                            || crate::random_values::is_spec(value)))
            }
            "mode" => matches!(value.as_str(), Some("all" | "any")),
            "result" => matches!(value.as_str(), Some("succeeded" | "needs_help")),
            "loop" => value.as_str().is_some_and(|id| id.len() <= 80),
            "profiles" | "samples" => value.as_array().is_some_and(|items| items.len() <= 40),
            "conditions" => value.as_array().is_some_and(|items| {
                items.len() <= 20
                    && items.iter().all(|item| {
                        item.as_object()
                            .is_some_and(|object| (3..=5).contains(&object.len()))
                            && matches!(item["source"].as_str(), Some("text" | "app" | "round"))
                            && matches!(
                                item["operator"].as_str(),
                                Some(
                                    "contains"
                                        | "not_contains"
                                        | "is"
                                        | "is_not"
                                        | "eq"
                                        | "gt"
                                        | "lt"
                                )
                            )
                            && item["value"]
                                .as_str()
                                .is_some_and(|text| text.chars().count() <= 500)
                            && item.get("guard").is_none_or(|guard| {
                                guard
                                    .as_str()
                                    .is_some_and(|text| text.chars().count() <= 500)
                            })
                    })
            }),
            _ => false,
        };
        anyhow::ensure!(valid, "步骤设置字段无效：{key}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        serde_json::from_value(json!({
            "id":uuid::Uuid::new_v4().to_string(),"revision":0,"name":"测试草稿",
            "description":"","entry":"a","viewport":[0,0,1],
            "nodes":[{"id":"a","kind":"wait","label":"等待","config":{},"targets":{},"position":[0,0]}]
        })).unwrap()
    }

    #[test]
    fn unfinished_graph_is_a_valid_draft() {
        validate(&draft()).unwrap();
    }

    #[test]
    fn draft_rejects_dangling_and_invalid_ports() {
        let mut value = draft();
        value.nodes[0]
            .targets
            .insert("next".into(), "missing".into());
        assert!(validate(&value).is_err());
        value.nodes[0].targets = HashMap::from([("other".into(), "a".into())]);
        assert!(validate(&value).is_err());
    }

    #[test]
    fn draft_rejects_malformed_conditions() {
        let mut value = draft();
        value.nodes[0].config = json!({"conditions":"invalid"});
        assert!(validate(&value).is_err());
    }
}
