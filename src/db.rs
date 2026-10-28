use anyhow::Result;
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
    TransactionTrait,
};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;

pub struct Store(DatabaseConnection);
impl Store {
    pub async fn open(path: &Path) -> Result<Self> {
        let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        options.sqlx_logging(false);
        let db = Database::connect(options).await?;
        for sql in [
            "PRAGMA journal_mode=WAL",
            "PRAGMA busy_timeout=5000",
            "CREATE TABLE IF NOT EXISTS records (kind TEXT NOT NULL,id TEXT NOT NULL,body TEXT NOT NULL,PRIMARY KEY(kind,id))",
            "CREATE UNIQUE INDEX IF NOT EXISTS one_run_per_phone_v2 ON records(json_extract(body,'$.device_id')) WHERE kind='runs' AND json_extract(body,'$.state') IN ('running','waiting_device','recovering','paused')",
            "DROP INDEX IF EXISTS one_run_per_phone",
        ] {
            db.execute(Statement::from_string(DbBackend::Sqlite, sql))
                .await?;
        }
        let migration = db.begin().await?;
        for sql in [
            // 旧数据按版本号保留最新内容，改为每个任务编号只存一份。
            "DELETE FROM records WHERE rowid IN (SELECT rowid FROM (SELECT rowid, ROW_NUMBER() OVER (PARTITION BY json_extract(body,'$.id') ORDER BY COALESCE(json_extract(body,'$.version'),0) DESC,rowid DESC) AS position FROM records WHERE kind='workflows') WHERE position>1)",
            "UPDATE records SET id=json_extract(body,'$.id'),body=json_remove(body,'$.version','$.active','$.sample_validated') WHERE kind='workflows'",
            "DELETE FROM records WHERE kind IN ('active','vision_validated')",
            // 取消任务归档后保留任务和定时内容，不重新开启已停用的定时。
            "DELETE FROM records WHERE kind='archived_workflows'",
        ] {
            migration
                .execute(Statement::from_string(DbBackend::Sqlite, sql))
                .await?;
        }
        migration.commit().await?;
        Ok(Self(db))
    }
    pub async fn put<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> Result<()> {
        let body = serde_json::to_string(value)?;
        let statement = Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO records(kind,id,body) VALUES (?,?,?) ON CONFLICT(kind,id) DO UPDATE SET body=excluded.body",
            [kind.into(), id.into(), body.into()],
        );
        self.0.execute(statement).await?;
        Ok(())
    }
    pub async fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<Option<T>> {
        self.0
            .query_one(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT body FROM records WHERE kind=? AND id=?",
                [kind.into(), id.into()],
            ))
            .await?
            .map(|r| Ok(serde_json::from_str(&r.try_get::<String>("", "body")?)?))
            .transpose()
    }
    pub async fn list<T: DeserializeOwned>(&self, kind: &str) -> Result<Vec<T>> {
        self.0
            .query_all(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT body FROM records WHERE kind=? ORDER BY rowid DESC",
                [kind.into()],
            ))
            .await?
            .into_iter()
            .map(|r| Ok(serde_json::from_str(&r.try_get::<String>("", "body")?)?))
            .collect()
    }
    pub async fn delete(&self, kind: &str, id: &str) -> Result<()> {
        self.0
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "DELETE FROM records WHERE kind=? AND id=?",
                [kind.into(), id.into()],
            ))
            .await?;
        Ok(())
    }

    pub async fn workflow_export_source(
        &self,
        id: &str,
    ) -> Result<Option<(crate::types::Workflow, Option<crate::drafts::Draft>)>> {
        let transaction = self.0.begin().await?;
        let read = |kind: &str, key: &str| {
            Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT body FROM records WHERE kind=? AND id=?",
                [kind.into(), key.into()],
            )
        };
        let Some(row) = transaction.query_one(read("workflows", id)).await? else {
            transaction.commit().await?;
            return Ok(None);
        };
        let workflow = serde_json::from_str(&row.try_get::<String>("", "body")?)?;
        let draft = if let Some(uuid) = id.strip_prefix("manual-") {
            transaction
                .query_one(read("workflow_drafts", uuid))
                .await?
                .map(|row| {
                    Ok::<_, anyhow::Error>(serde_json::from_str(
                        &row.try_get::<String>("", "body")?,
                    )?)
                })
                .transpose()?
        } else {
            None
        };
        transaction.commit().await?;
        Ok(Some((workflow, draft)))
    }

    pub async fn insert_workflow_import(
        &self,
        operation: &str,
        digest: &str,
        result: &serde_json::Value,
        workflow: Option<&crate::types::Workflow>,
        draft: Option<&crate::drafts::Draft>,
    ) -> Result<()> {
        let transaction = self.0.begin().await?;
        let mut records = Vec::new();
        if let Some(workflow) = workflow {
            records.push((
                "workflows",
                workflow.id.as_str(),
                serde_json::to_string(workflow)?,
            ));
        }
        if let Some(draft) = draft {
            records.push((
                "workflow_drafts",
                draft.id.as_str(),
                serde_json::to_string(draft)?,
            ));
        }
        records.push((
            "workflow_imports",
            operation,
            serde_json::to_string(&serde_json::json!({"digest":digest,"result":result}))?,
        ));
        for (kind, id, body) in records {
            transaction
                .execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT INTO records(kind,id,body) VALUES (?,?,?)",
                    [kind.into(), id.into(), body.into()],
                ))
                .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn remove_workflow(&self, id: &str, draft_id: &str) -> Result<()> {
        let transaction = self.0.begin().await?;
        let mut run_ids = std::collections::BTreeSet::<String>::new();
        for row in transaction.query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT id FROM records WHERE kind IN ('runs','archived_runs') AND json_extract(body,'$.workflow.id')=?",
            [id.into()],
        )).await? {
            run_ids.insert(row.try_get("", "id")?);
        }
        if let Some(row) = transaction
            .query_one(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT body FROM records WHERE kind='workflow_cleanup' AND id=?",
                [id.into()],
            ))
            .await?
        {
            let pending: crate::workflow_cleanup::Pending =
                serde_json::from_str(&row.try_get::<String>("", "body")?)?;
            run_ids.extend(pending.run_ids);
        }
        transaction.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "DELETE FROM records WHERE (kind IN ('events','archived_events','vision_results') AND json_extract(body,'$.run_id') IN (SELECT id FROM records WHERE kind IN ('runs','archived_runs') AND json_extract(body,'$.workflow.id')=?)) OR (kind='vision_results' AND json_extract(body,'$.workflow_id')=?) OR (kind='vision_validations' AND (json_extract(body,'$.workflow')=? OR json_extract(body,'$.workflow') GLOB ?))",
            [id.into(), id.into(), id.into(), format!("{id}@*").into()],
        )).await?;
        transaction.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "DELETE FROM records WHERE kind IN ('runs','archived_runs') AND json_extract(body,'$.workflow.id')=?",
            [id.into()],
        )).await?;
        transaction
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "DELETE FROM records WHERE (kind='workflows' AND id=?) OR (kind='workflow_drafts' AND id=?) OR (kind='schedules' AND json_extract(body,'$.workflow_id')=?)",
                [id.into(), draft_id.into(), id.into()],
            ))
            .await?;
        // 防止重启时重新生成用户已经移除的内置任务。
        transaction
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO records(kind,id,body) VALUES ('removed_workflows',?,?) ON CONFLICT(kind,id) DO UPDATE SET body=excluded.body",
                [id.into(), serde_json::to_string(id)?.into()],
            ))
            .await?;
        if !run_ids.is_empty() {
            let pending = crate::workflow_cleanup::Pending {
                workflow_id: id.into(),
                run_ids: run_ids.into_iter().collect(),
            };
            transaction.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO records(kind,id,body) VALUES ('workflow_cleanup',?,?) ON CONFLICT(kind,id) DO UPDATE SET body=excluded.body",
                [id.into(), serde_json::to_string(&pending)?.into()],
            )).await?;
        }
        transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[tokio::test]
    async fn deletes_target_history_atomically_and_preserves_other_tasks() -> Result<()> {
        let folder =
            std::env::temp_dir().join(format!("adb-delete-workflow-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder)?;
        let path = folder.join("data.sqlite");
        let store = Store::open(&path).await?;
        let current = uuid::Uuid::new_v4().to_string();
        let archived = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        for task in ["target", "other"] {
            store.put("workflows", task, &json!({"id":task})).await?;
            store
                .put("workflow_drafts", task, &json!({"id":task}))
                .await?;
            store
                .put("schedules", task, &json!({"workflow_id":task}))
                .await?;
            store
                .put("vision_validations", task, &json!({"workflow":task}))
                .await?;
        }
        for (run, task, kind) in [
            (&current, "target", "runs"),
            (&archived, "target", "archived_runs"),
            (&other, "other", "runs"),
        ] {
            store
                .put(kind, run, &json!({"workflow":{"id":task}}))
                .await?;
            for child in ["events", "archived_events", "vision_results"] {
                store.put(child, run, &json!({"run_id":run})).await?;
            }
        }
        store
            .put(
                "vision_validations",
                "legacy-target",
                &json!({"workflow":"target@7"}),
            )
            .await?;
        store
            .put(
                "vision_validations",
                "legacy-other",
                &json!({"workflow":"target-extra@7"}),
            )
            .await?;
        store
            .put(
                "vision_results",
                "target-orphan",
                &json!({"workflow_id":"target","run_id":"expired"}),
            )
            .await?;
        store
            .put(
                "vision_checks",
                "standalone",
                &json!({"image":"standalone.png"}),
            )
            .await?;
        store.0.execute(Statement::from_string(DbBackend::Sqlite,
            "CREATE TRIGGER fail_delete BEFORE DELETE ON records WHEN OLD.kind='workflows' AND OLD.id='target' BEGIN SELECT RAISE(ABORT, 'test rollback'); END".to_string())).await?;
        assert!(store.remove_workflow("target", "target").await.is_err());
        assert!(store.get::<Value>("runs", &current).await?.is_some());
        assert!(store.get::<Value>("events", &current).await?.is_some());
        assert!(store.get::<Value>("workflows", "target").await?.is_some());
        assert!(
            store
                .get::<Value>("workflow_cleanup", "target")
                .await?
                .is_none()
        );
        store
            .0
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "DROP TRIGGER fail_delete".to_string(),
            ))
            .await?;
        store.remove_workflow("target", "target").await?;
        for kind in [
            "workflows",
            "workflow_drafts",
            "schedules",
            "vision_validations",
        ] {
            assert!(
                store.get::<Value>(kind, "target").await?.is_none(),
                "{kind}"
            );
            assert!(store.get::<Value>(kind, "other").await?.is_some(), "{kind}");
        }
        for kind in [
            "runs",
            "archived_runs",
            "events",
            "archived_events",
            "vision_results",
        ] {
            assert!(
                store.get::<Value>(kind, &current).await?.is_none(),
                "{kind}"
            );
            assert!(
                store.get::<Value>(kind, &archived).await?.is_none(),
                "{kind}"
            );
        }
        for kind in ["runs", "events", "archived_events", "vision_results"] {
            assert!(store.get::<Value>(kind, &other).await?.is_some(), "{kind}");
        }
        assert!(
            store
                .get::<Value>("vision_validations", "legacy-target")
                .await?
                .is_none()
        );
        assert!(
            store
                .get::<Value>("vision_validations", "legacy-other")
                .await?
                .is_some()
        );
        assert!(
            store
                .get::<Value>("vision_results", "target-orphan")
                .await?
                .is_none()
        );
        assert!(
            store
                .get::<Value>("vision_checks", "standalone")
                .await?
                .is_some()
        );
        store.0.close().await?;
        let store = Store::open(&path).await?;
        let pending = store
            .get::<crate::workflow_cleanup::Pending>("workflow_cleanup", "target")
            .await?
            .unwrap();
        assert_eq!(pending.run_ids.len(), 2);
        assert!(pending.run_ids.contains(&current) && pending.run_ids.contains(&archived));
        assert!(
            store
                .get::<Value>("removed_workflows", "target")
                .await?
                .is_some()
        );
        store.0.close().await?;
        std::fs::remove_dir_all(folder)?;
        Ok(())
    }

    #[tokio::test]
    async fn legacy_workflows_keep_only_latest_and_overwrite_in_place() -> Result<()> {
        let folder = std::env::temp_dir().join(format!("adb-workflow-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder)?;
        let path = folder.join("data.sqlite");
        let store = Store::open(&path).await?;
        store
            .put(
                "workflows",
                "task@10",
                &json!({"id":"task","version":10,"name":"最新内容"}),
            )
            .await?;
        store
            .put(
                "workflows",
                "task@2",
                &json!({"id":"task","version":2,"name":"旧内容"}),
            )
            .await?;
        store
            .put(
                "workflows",
                "other",
                &json!({"id":"other","name":"另一项任务"}),
            )
            .await?;
        let history = json!({"workflow":{"id":"task","version":2},"state":"succeeded"});
        store.put("runs", "history", &history).await?;
        store.put("archived_workflows", "task", &"task").await?;
        let schedule = json!({"workflow_id":"task","enabled":false});
        store.put("schedules", "schedule", &schedule).await?;
        store.0.close().await?;

        let store = Store::open(&path).await?;
        assert_eq!(store.list::<Value>("workflows").await?.len(), 2);
        assert_eq!(
            store.get::<Value>("workflows", "task").await?,
            Some(json!({"id":"task","name":"最新内容"}))
        );
        assert!(store.get::<Value>("workflows", "task@10").await?.is_none());
        assert_eq!(store.get::<Value>("runs", "history").await?, Some(history));
        assert_eq!(
            store
                .get::<String>("archived_workflows", "task")
                .await?
                .as_deref(),
            None
        );
        assert_eq!(
            store.get::<Value>("schedules", "schedule").await?,
            Some(schedule)
        );
        store
            .put("workflows", "task", &json!({"id":"task","name":"覆盖保存"}))
            .await?;
        store.0.close().await?;

        let store = Store::open(&path).await?;
        assert_eq!(store.list::<Value>("workflows").await?.len(), 2);
        assert_eq!(
            store.get::<Value>("workflows", "task").await?,
            Some(json!({"id":"task","name":"覆盖保存"}))
        );
        store.0.close().await?;
        std::fs::remove_dir_all(folder)?;
        Ok(())
    }
}
