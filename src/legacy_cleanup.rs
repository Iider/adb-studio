use crate::{db::Store, workflow_cleanup::Pending};
use anyhow::{Context, Result};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

// 只识别要删除的数据，不把旧任务反序列化为可运行任务。
fn editor_workflow(value: &Value) -> bool {
    value["id"]
        .as_str()
        .and_then(|id| id.strip_prefix("manual-"))
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        && value["nodes"].as_array().is_some_and(|nodes| {
            !nodes.is_empty()
                && nodes
                    .iter()
                    .all(|node| matches!(node["type"].as_str(), Some("canvas" | "end")))
        })
}

pub async fn purge(store: &Store, path: &Path) -> Result<()> {
    let mut workflows = BTreeSet::new();
    let mut runs = BTreeSet::new();
    let mut pending = BTreeMap::<String, Vec<String>>::new();
    let mut retained = BTreeSet::new();
    for value in store.list::<Value>("workflows").await? {
        if !editor_workflow(&value) {
            workflows.insert(value["id"].as_str().context("旧任务缺少编号")?.to_owned());
        } else {
            retained.insert(value["id"].as_str().unwrap().to_owned());
        }
    }
    for kind in ["runs", "archived_runs"] {
        for value in store.list::<Value>(kind).await? {
            let workflow = &value["workflow"];
            if !editor_workflow(workflow)
                || workflows.contains(workflow["id"].as_str().unwrap_or(""))
            {
                let id = value["id"]
                    .as_str()
                    .context("旧执行记录缺少编号")?
                    .to_owned();
                runs.insert(id.clone());
                pending
                    .entry(workflow["id"].as_str().unwrap_or("legacy").to_owned())
                    .or_default()
                    .push(id);
            }
        }
    }
    let schedules = store.list::<Value>("schedules").await?;
    let orphan_schedules: Vec<_> = schedules
        .iter()
        .filter(|value| !retained.contains(value["workflow_id"].as_str().unwrap_or("")))
        .map(|value| {
            value["id"]
                .as_str()
                .context("旧定时缺少编号")
                .map(str::to_owned)
        })
        .collect::<Result<_>>()?;
    let old_fields = store.0.query_one(Statement::from_string(DbBackend::Sqlite,
        "SELECT COUNT(*) AS total FROM records WHERE kind IN ('removed_workflows','archived_workflows','active','vision_validated','vision_validations') OR (kind='devices' AND json_type(body,'$.tap_point') IS NOT NULL) OR (kind='workflows' AND json_type(body,'$.launch_mode') IS NOT NULL) OR (kind='schedules' AND (json_type(body,'$.tap') IS NOT NULL OR json_type(body,'$.swipe') IS NOT NULL))".to_string()))
        .await?.context("无法检查旧任务字段")?.try_get::<i64>("", "total")?;
    if workflows.is_empty() && runs.is_empty() && orphan_schedules.is_empty() && old_fields == 0 {
        return Ok(());
    }
    let backup = path.with_file_name(format!(
        "before-editor-only-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    store
        .0
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "VACUUM INTO ?",
            [backup.to_string_lossy().to_string().into()],
        ))
        .await?;
    let existing = store.list::<Pending>("workflow_cleanup").await?;
    let transaction = store.0.begin().await?;
    for id in orphan_schedules {
        transaction
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "DELETE FROM records WHERE kind='schedules' AND id=?",
                [id.into()],
            ))
            .await?;
    }
    for id in &workflows {
        transaction.execute(Statement::from_sql_and_values(DbBackend::Sqlite,
            "DELETE FROM records WHERE (kind='workflows' AND json_extract(body,'$.id')=?) OR (kind='schedules' AND json_extract(body,'$.workflow_id')=?) OR (kind='vision_results' AND json_extract(body,'$.workflow_id')=?)",
            [id.clone().into(), id.clone().into(), id.clone().into()])).await?;
    }
    for id in &runs {
        transaction.execute(Statement::from_sql_and_values(DbBackend::Sqlite,
            "DELETE FROM records WHERE (kind IN ('runs','archived_runs') AND id=?) OR (kind IN ('events','archived_events','vision_results') AND json_extract(body,'$.run_id')=?)",
            [id.clone().into(), id.clone().into()])).await?;
    }
    for (workflow_id, run_ids) in pending {
        let mut value = Pending {
            workflow_id,
            run_ids,
        };
        if let Some(existing) = existing.iter().find(|p| p.workflow_id == value.workflow_id) {
            value.run_ids.extend(existing.run_ids.clone());
        }
        value.run_ids.sort();
        value.run_ids.dedup();
        transaction.execute(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO records(kind,id,body) VALUES ('workflow_cleanup',?,?) ON CONFLICT(kind,id) DO UPDATE SET body=excluded.body",
            [value.workflow_id.clone().into(), serde_json::to_string(&value)?.into()])).await?;
    }
    transaction.execute(Statement::from_string(DbBackend::Sqlite,
        "DELETE FROM records WHERE kind IN ('removed_workflows','archived_workflows','active','vision_validated','vision_validations')".to_string())).await?;
    for sql in [
        "UPDATE records SET body=json_remove(body,'$.tap_point') WHERE kind='devices' AND json_type(body,'$.tap_point') IS NOT NULL",
        "UPDATE records SET body=json_remove(body,'$.tap','$.swipe') WHERE kind='schedules'",
        "UPDATE records SET body=json_remove(body,'$.launch_mode') WHERE kind='workflows'",
        "UPDATE records SET body=json_remove(body,'$.workflow.launch_mode') WHERE kind IN ('runs','archived_runs')",
    ] {
        transaction
            .execute(Statement::from_string(DbBackend::Sqlite, sql.to_string()))
            .await?;
    }
    transaction.commit().await?;
    tracing::info!(
        "已清理 {} 项旧任务、{} 条旧执行记录；备份：{}",
        workflows.len(),
        runs.len(),
        backup.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn purges_legacy_before_deserialization_and_preserves_editor_data() -> Result<()> {
        let folder = std::env::temp_dir().join(format!("adb-editor-only-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder)?;
        let path = folder.join("data.sqlite");
        let store = Store::open(&path).await?;
        let uuid = uuid::Uuid::new_v4().to_string();
        let id = format!("manual-{uuid}");
        let editor = json!({"id":id,"nodes":[{"type":"canvas"},{"type":"end"}]});
        store.put("workflows", &id, &editor).await?;
        store
            .put("workflow_drafts", &uuid, &json!({"id":uuid,"nodes":[]}))
            .await?;
        store
            .put("devices", "phone", &json!({"id":"phone"}))
            .await?;
        store
            .put(
                "workflows",
                "swipe@2",
                &json!({"id":"swipe","nodes":[{"type":"swipe_loop"}]}),
            )
            .await?;
        let old_run = uuid::Uuid::new_v4().to_string();
        store
            .put(
                "runs",
                &old_run,
                &json!({"id":old_run,"workflow":{"id":"swipe","nodes":[{"type":"swipe_loop"}]}}),
            )
            .await?;
        store
            .put(
                "archived_runs",
                "orphan",
                &json!({"id":"orphan","workflow":{"id":"deleted","nodes":[{"type":"vision_tap"}]}}),
            )
            .await?;
        store
            .put(
                "runs",
                "editor-run",
                &json!({"id":"editor-run","workflow":editor}),
            )
            .await?;
        store
            .put("events", "old", &json!({"run_id":old_run}))
            .await?;
        store
            .put("events", "new", &json!({"run_id":"editor-run"}))
            .await?;
        store
            .put(
                "schedules",
                "old",
                &json!({"id":"old","workflow_id":"swipe"}),
            )
            .await?;
        store
            .put("schedules", "new", &json!({"id":"new","workflow_id":id}))
            .await?;
        store.0.close().await?;
        let store = Store::open(&path).await?;
        assert_eq!(store.list::<Value>("workflows").await?, vec![editor]);
        assert_eq!(store.list::<Value>("runs").await?.len(), 1);
        assert!(store.list::<Value>("archived_runs").await?.is_empty());
        assert!(store.get::<Value>("events", "old").await?.is_none());
        assert!(store.get::<Value>("events", "new").await?.is_some());
        assert_eq!(store.list::<Value>("schedules").await?.len(), 1);
        assert_eq!(store.list::<Value>("devices").await?.len(), 1);
        assert_eq!(store.list::<Value>("workflow_drafts").await?.len(), 1);
        assert_eq!(
            store
                .get::<Pending>("workflow_cleanup", "swipe")
                .await?
                .unwrap()
                .run_ids,
            vec![old_run]
        );
        let backups = || {
            std::fs::read_dir(&folder)
                .unwrap()
                .filter_map(|p| p.ok())
                .filter(|p| {
                    p.file_name()
                        .to_string_lossy()
                        .starts_with("before-editor-only-")
                })
                .count()
        };
        assert_eq!(backups(), 1);
        purge(&store, &path).await?;
        assert_eq!(backups(), 1);
        store.0.close().await?;
        std::fs::remove_dir_all(folder)?;
        Ok(())
    }

    #[tokio::test]
    async fn failure_rolls_back_all_deletions_and_keeps_backup() -> Result<()> {
        let folder =
            std::env::temp_dir().join(format!("adb-purge-rollback-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder)?;
        let path = folder.join("data.sqlite");
        let store = Store::open(&path).await?;
        let old = json!({"id":"swipe","nodes":[{"type":"swipe_loop"}]});
        store.put("workflows", "swipe", &old).await?;
        store
            .put(
                "schedules",
                "orphan",
                &json!({"id":"orphan","workflow_id":"deleted"}),
            )
            .await?;
        store.0.execute(Statement::from_string(DbBackend::Sqlite,
            "CREATE TRIGGER fail_purge BEFORE DELETE ON records WHEN OLD.kind='workflows' BEGIN SELECT RAISE(ABORT,'failure'); END".to_string())).await?;
        assert!(purge(&store, &path).await.is_err());
        assert_eq!(store.get::<Value>("workflows", "swipe").await?, Some(old));
        assert!(store.get::<Value>("schedules", "orphan").await?.is_some());
        let backup = std::fs::read_dir(&folder)?
            .filter_map(|p| p.ok())
            .find(|p| {
                p.file_name()
                    .to_string_lossy()
                    .starts_with("before-editor-only-")
            })
            .unwrap();
        // Read the backup directly: opening Store would apply cleanup again.
        let db =
            sea_orm::Database::connect(format!("sqlite://{}?mode=ro", backup.path().display()))
                .await?;
        let row = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS total FROM records WHERE kind='workflows'".to_string(),
            ))
            .await?
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "total")?, 1);
        db.close().await?;
        store.0.close().await?;
        std::fs::remove_dir_all(folder)?;
        Ok(())
    }
}
