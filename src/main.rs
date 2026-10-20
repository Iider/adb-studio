mod adb;
mod api;
mod db;
mod engine;
mod recognition;
mod run_control;
mod schedule;
mod types;

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
    stop: CancellationToken,
    controls: Mutex<std::collections::HashMap<String, Arc<run_control::RunControl>>>,
    workers: tokio_util::task::TaskTracker,
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
    let root = std::env::var_os("ADB_STUDIO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let data = std::env::var_os("ADB_STUDIO_DATA")
        .map(PathBuf::from)
        .unwrap_or(root.join("data"));
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
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600)).await?;
        token
    };
    let (events, _) = broadcast::channel(128);
    let app = Arc::new(App {
        db: db::Store::open(&data.join("data.sqlite")).await?,
        adb: adb::Adb::new(
            std::env::var_os("ADB_STUDIO_ADB")
                .map(PathBuf::from)
                .unwrap_or(root.join("../platform-tools/adb")),
        ),
        root,
        data,
        token,
        events,
        run_gate: Mutex::new(()),
        device_gate: Mutex::new(()),
        schedule_gate: Mutex::new(()),
        stop: CancellationToken::new(),
        controls: Mutex::new(Default::default()),
        workers: tokio_util::task::TaskTracker::new(),
    });
    for (id, name, model) in [
        ("3430131496001EB", "iQOO", "V2171A"),
        ("BGR6R20610013719", "荣耀", "ASK-AL00x"),
    ] {
        if app.db.get::<Device>("devices", id).await?.is_none() {
            app.db
                .put("devices", id, &Device::new(id, name, model))
                .await?;
        }
    }
    for mut d in app.db.list::<Device>("devices").await? {
        d.online = false;
        d.endpoints.clear();
        app.db.put("devices", &d.id, &d).await?;
    }
    for wf in engine::builtins() {
        if app
            .db
            .get::<Workflow>("workflows", &wf.key())
            .await?
            .is_none()
        {
            app.db.put("workflows", &wf.key(), &wf).await?;
        }
    }
    if let Err(error) = adb::refresh(&app).await {
        tracing::warn!("初次设备检查：{error}");
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
