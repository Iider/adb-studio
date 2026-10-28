use crate::App;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{io::ErrorKind, path::Path};

#[derive(Serialize, Deserialize)]
pub struct Pending {
    pub workflow_id: String,
    pub run_ids: Vec<String>,
}

async fn remove_shots(data: &Path, run_ids: &[String]) -> Result<()> {
    ensure!(
        run_ids.iter().all(|id| uuid::Uuid::parse_str(id).is_ok()),
        "执行记录的截图目录编号无效"
    );
    for id in run_ids {
        match tokio::fs::remove_dir_all(data.join("shots").join(id)).await {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("截图清理失败"),
        }
    }
    Ok(())
}

// 调用方持有 run_gate，避免即时清理与后台重试同时修改清理记录。
pub async fn finish(app: &App, workflow_id: &str) -> Result<bool> {
    let Some(pending) = app
        .db
        .get::<Pending>("workflow_cleanup", workflow_id)
        .await?
    else {
        return Ok(true);
    };
    if let Err(error) = remove_shots(&app.data, &pending.run_ids).await {
        tracing::warn!("任务 {workflow_id} 的截图稍后重试清理：{error}");
        return Ok(false);
    }
    app.db.delete("workflow_cleanup", workflow_id).await?;
    Ok(true)
}

pub async fn retry(app: &App) -> Result<()> {
    let _gate = app.run_gate.lock().await;
    for pending in app.db.list::<Pending>("workflow_cleanup").await? {
        finish(app, &pending.workflow_id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn removes_only_target_shots_and_can_retry_missing_or_blocked_folders() -> Result<()> {
        let data = std::env::temp_dir().join(format!("adb-delete-shots-{}", uuid::Uuid::new_v4()));
        let own = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        tokio::fs::create_dir_all(data.join("shots").join(&own)).await?;
        tokio::fs::create_dir_all(data.join("shots").join(&other)).await?;
        tokio::fs::write(data.join("shots").join(&own).join("sample.png"), b"own").await?;
        tokio::fs::write(data.join("shots").join(&other).join("sample.png"), b"other").await?;
        remove_shots(&data, std::slice::from_ref(&own)).await?;
        remove_shots(&data, std::slice::from_ref(&own)).await?;
        assert!(!data.join("shots").join(&own).exists());
        assert!(data.join("shots").join(&other).join("sample.png").exists());
        assert!(remove_shots(&data, &["../outside".into()]).await.is_err());
        tokio::fs::write(data.join("shots").join(&own), b"blocked").await?;
        assert!(
            remove_shots(&data, std::slice::from_ref(&own))
                .await
                .is_err()
        );
        tokio::fs::remove_file(data.join("shots").join(&own)).await?;
        remove_shots(&data, &[own]).await?;
        tokio::fs::remove_dir_all(data).await?;
        Ok(())
    }
}
