mod access;
mod adb;
mod api;
mod canvas;
mod condition_model;
mod db;
mod drafts;
mod engine;
mod legacy_cleanup;
mod mirror;
mod platform;
mod recognition;
mod run_control;
mod schedule;
mod step_test;
mod types;
mod vision;
mod workflow_cleanup;
mod workflow_inputs;
mod workflow_transfer;

use anyhow::Result;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use types::*;

pub struct App {
    db: db::Store,
    adb: adb::Adb,
    root: PathBuf,
    data: PathBuf,
    token: String,
    events: broadcast::Sender<serde_json::Value>,
    run_gate: Mutex<()>,
    device_gate: Mutex<()>,
    schedule_gate: Mutex<()>,
    mirrors: Mutex<std::collections::HashMap<String, Arc<mirror::Slot>>>,
    manual_devices: Mutex<std::collections::HashSet<String>>,
    stop: CancellationToken,
    controls: Mutex<std::collections::HashMap<String, Arc<run_control::RunControl>>>,
    workers: tokio_util::task::TaskTracker,
    vision_checks: tokio::sync::Semaphore,
    access: access::Policy,
    model_client: reqwest::Client,
}

impl App {
    async fn event(&self, run: &Run, message: &str) -> Result<()> {
        let event = serde_json::json!({"id":uuid::Uuid::new_v4().to_string(),"run_id":run.id,"at":now(),"node":run.node,"message":message});
        self.db
            .put("events", event["id"].as_str().unwrap(), &event)
            .await?;
        let _ = self
            .events
            .send(serde_json::json!({"type":"changed","run_id":run.id}));
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let root = platform::root();
    let data = std::env::var_os("ADB_STUDIO_DATA")
        .map(PathBuf::from)
        .unwrap_or(root.join("data"));
    tokio::fs::create_dir_all(&data).await?;
    #[cfg(windows)]
    platform::private_directory(&data).await?;
    tokio::fs::create_dir_all(data.join("shots")).await?;
    let token_file = data.join("access-token");
    let token = if token_file.exists() {
        tokio::fs::read_to_string(&token_file)
            .await?
            .trim()
            .to_owned()
    } else {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        tokio::fs::write(&token_file, &token).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600)).await?;
        }
        token
    };
    let (events, _) = broadcast::channel(128);
    let app = Arc::new(App {
        db: db::Store::open(&data.join("data.sqlite")).await?,
        adb: adb::Adb::new(platform::adb(&root)),
        root,
        data,
        token,
        events,
        run_gate: Mutex::new(()),
        device_gate: Mutex::new(()),
        schedule_gate: Mutex::new(()),
        mirrors: Mutex::new(Default::default()),
        manual_devices: Mutex::new(Default::default()),
        stop: CancellationToken::new(),
        controls: Mutex::new(Default::default()),
        workers: tokio_util::task::TaskTracker::new(),
        vision_checks: tokio::sync::Semaphore::new(1),
        model_client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()?,
        access: access::Policy::new(
            &std::env::var("ADB_STUDIO_ORIGINS").unwrap_or_default(),
            std::env::var("ADB_STUDIO_HOST").as_deref() == Ok("0.0.0.0"),
        )?,
    });
    workflow_cleanup::retry(&app).await?;
    for mut d in app.db.list::<Device>("devices").await? {
        d.online = false;
        d.endpoints.clear();
        app.db.put("devices", &d.id, &d).await?;
    }
    if let Err(error) = adb::refresh(&app).await {
        tracing::warn!("初次设备检查：{error}");
    }
    if let Err(error) = workflow_transfer::cleanup_imports(&app).await {
        tracing::warn!("导入临时文件等待下次清理：{error}");
    }
    engine::recover(app.clone()).await?;
    let monitor = tokio::spawn(adb::monitor(app.clone()));
    let scheduler = tokio::spawn(schedule::monitor(app.clone()));
    let port = std::env::var("ADB_STUDIO_PORT")
        .unwrap_or_else(|_| "17841".into())
        .parse::<u16>()?;
    let host = std::env::var("ADB_STUDIO_HOST")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse::<std::net::Ipv4Addr>()?;
    let bind = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("工作台：http://{bind}");
    let shutdown = app.stop.clone();
    let signal = app.stop.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
        }
        #[cfg(windows)]
        {
            let mut interrupt = tokio::signal::windows::ctrl_break().expect("注册停止信号失败");
            let stop_file = std::env::var_os("ADB_STUDIO_STOP_FILE").map(PathBuf::from);
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = interrupt.recv() => {},
                _ = async {
                    loop {
                        if stop_file.as_ref().is_some_and(|p| p.is_file()) { break; }
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                } => {},
            }
        }
        signal.cancel();
    });
    axum::serve(listener, api::router(app.clone()))
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await?;
    monitor.abort();
    scheduler.abort();
    app.workers.close();
    app.workers.wait().await;
    Ok(())
}
