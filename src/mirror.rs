use crate::App;
use anyhow::{Context, Result, bail};
use axum::extract::ws::{Message, WebSocket};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::io::AsyncReadExt;

pub async fn serve(app: Arc<App>, id: String, mut socket: WebSocket) {
    let result = stream(&app, &id, &mut socket).await;
    if let Err(error) = result {
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            socket.send(Message::Text(
                json!({"error":error.to_string()}).to_string().into(),
            )),
        )
        .await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Close(None))).await;
}

async fn stream(app: &App, id: &str, socket: &mut WebSocket) -> Result<()> {
    let slot = app
        .mirrors
        .lock()
        .await
        .entry(id.to_owned())
        .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(1)))
        .clone();
    let _permit = slot
        .try_acquire_owned()
        .context("这台手机已在其他页面投屏，请先关闭原投屏")?;
    let server = app.data.join("bin/scrcpy-server-v3.1");
    let mut connection = tokio::select! {
        result = app.adb.video(id, &server, app.workers.clone()) => result?,
        _ = app.stop.cancelled() => return Ok(()),
        _ = socket.recv() => return Ok(()),
    };
    let stream = connection.stream.as_mut().context("投屏通道未连接")?;
    let mut buffer = [0u8; 64 * 1024];
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    let mut last_seen = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = app.stop.cancelled() => return Ok(()),
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Pong(_))) => last_seen = tokio::time::Instant::now(),
                Some(Ok(Message::Close(_))) | None => return Ok(()),
                Some(Err(error)) => return Err(error.into()),
                Some(Ok(_)) => bail!("当前投屏仅供观看"),
            },
            _ = heartbeat.tick() => {
                if last_seen.elapsed() > Duration::from_secs(15) { bail!("投屏连接已中断，请重新开启") }
                tokio::time::timeout(Duration::from_secs(3), socket.send(Message::Ping(Vec::new().into()))).await??;
            },
            count = stream.read(&mut buffer) => {
                let count = count.context("手机投屏连接中断")?;
                if count == 0 { bail!("手机投屏已结束，请检查连接后重新开启") }
                tokio::time::timeout(Duration::from_secs(3), socket.send(Message::Binary(buffer[..count].to_vec().into())))
                    .await.context("画面传输过慢，请重新开启投屏")??;
            }
        }
    }
}
