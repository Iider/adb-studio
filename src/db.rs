use anyhow::Result;
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
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
}
