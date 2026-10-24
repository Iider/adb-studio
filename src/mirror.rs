use crate::{App, types::Run};
use anyhow::{Context, Result, bail, ensure};
use axum::extract::ws::{Message, WebSocket};
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub async fn ensure_automatic(app: &App, id: &str) -> Result<()> {
    ensure!(
        !app.manual_devices.lock().await.contains(id),
        "这台手机正在手动操作，请先在投屏中结束操作"
    );
    Ok(())
}

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

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Control { enabled: bool },
}

#[derive(Default)]
struct Control {
    enabled: bool,
    poisoned: bool,
    touch: Option<Vec<u8>>,
    key_up: Option<Vec<u8>>,
}
impl Control {
    fn validate(&self, data: &[u8]) -> Result<()> {
        ensure!(self.enabled, "请先开启操作手机");
        match data.len() {
            2 => {
                ensure!(
                    data[0] == 10 && [0, 2].contains(&data[1]),
                    "屏幕电源消息无效"
                );
                ensure!(self.touch.is_none(), "请先松开触点，再切换手机屏幕");
            }
            32 => {
                ensure!(data[0] == 2 && data[2..10] == [0; 8], "只支持单指触控");
                let action = data[1];
                ensure!(action <= 3, "触控动作无效");
                ensure!(
                    (action == 0) == self.touch.is_none(),
                    "触控顺序无效，请重新开启操作"
                );
                let width = u16::from_be_bytes(data[18..20].try_into()?);
                let height = u16::from_be_bytes(data[20..22].try_into()?);
                let x = u32::from_be_bytes(data[10..14].try_into()?);
                let y = u32::from_be_bytes(data[14..18].try_into()?);
                ensure!(
                    width > 0
                        && height > 0
                        && width <= 8192
                        && height <= 8192
                        && x < width as u32
                        && y < height as u32,
                    "触控位置超出画面"
                );
                let action_button = u32::from_be_bytes(data[24..28].try_into()?);
                let buttons = u32::from_be_bytes(data[28..32].try_into()?);
                ensure!(action_button <= 1 && buttons <= 1, "触控按键无效");
            }
            28 => {
                ensure!(self.touch.is_none(), "请先松开触点，再使用导航键");
                let down = &data[..14];
                let up = &data[14..];
                ensure!(
                    down[0..2] == [0, 0] && up[0..2] == [0, 1] && down[2..6] == up[2..6],
                    "导航键消息无效"
                );
                let key = u32::from_be_bytes(down[2..6].try_into()?);
                ensure!(
                    [3, 4, 187].contains(&key) && down[6..14] == [0; 8] && up[6..14] == [0; 8],
                    "仅支持返回、主页和最近任务"
                );
            }
            _ => bail!("操作消息无效"),
        }
        Ok(())
    }
    async fn inject(&mut self, stream: &mut TcpStream, data: &[u8]) -> Result<()> {
        self.validate(data)?;
        if data.len() == 32 && [0, 2].contains(&data[1]) {
            self.touch = Some(data.to_vec());
        }
        if data.len() == 28 {
            self.key_up = Some(data[14..].to_vec());
        }
        self.send(stream, data).await?;
        if data.len() == 32 && [1, 3].contains(&data[1]) {
            self.touch = None;
        }
        self.key_up = None;
        Ok(())
    }
    async fn send(&mut self, stream: &mut TcpStream, data: &[u8]) -> Result<()> {
        ensure!(!self.poisoned, "手机操作通道已中断");
        if let Err(error) = write(stream, data).await {
            // 部分消息可能已写入；继续追加会破坏控制协议的边界。
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }
    async fn release(&mut self, stream: &mut TcpStream) -> Result<()> {
        ensure!(!self.poisoned, "手机操作通道已中断");
        if let Some(mut cancel) = self.touch.clone() {
            cancel[1] = 3;
            cancel[22..32].fill(0);
            self.send(stream, &cancel).await?;
            self.touch = None;
        }
        if let Some(up) = self.key_up.clone() {
            self.send(stream, &up).await?;
            self.key_up = None;
        }
        self.enabled = false;
        Ok(())
    }
}
async fn write(stream: &mut TcpStream, data: &[u8]) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(2), stream.write_all(data))
        .await
        .context("手机操作响应超时，已结束操作，请检查连接后重试")?
        .context("手机操作连接中断")
}
async fn control_status(
    socket: &mut WebSocket,
    enabled: bool,
    error: Option<String>,
) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(2),
        socket.send(Message::Text(
            json!({"type":"control","enabled":enabled,"error":error})
                .to_string()
                .into(),
        )),
    )
    .await??;
    Ok(())
}
async fn acquire(app: &App, id: &str) -> Result<()> {
    let _gate = app.run_gate.lock().await;
    for run in app.db.list::<Run>("runs").await? {
        if run.device_id == id && run.busy() {
            ensure!(
                run.state == "paused" && !app.controls.lock().await.contains_key(&run.id),
                "任务仍在运行，请先暂停接管，等待暂停完成"
            );
        }
    }
    let display_off = app
        .db
        .get::<crate::types::Device>("devices", id)
        .await?
        .context("手机未登记")?
        .display_off;
    app.adb.check_unlocked_for_mirror(id, display_off).await?;
    app.manual_devices.lock().await.insert(id.to_owned());
    let _ = app.events.send(json!({"type":"changed"}));
    Ok(())
}
async fn free(app: &App, id: &str) {
    let _gate = app.run_gate.lock().await;
    app.manual_devices.lock().await.remove(id);
    let _ = app.events.send(json!({"type":"changed"}));
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
    let installed = app.data.join("bin/scrcpy-server-v3.1");
    let server = if installed.is_file() {
        installed
    } else {
        app.root.join("bin/scrcpy-server-v3.1")
    };
    let display_off = app
        .db
        .get::<crate::types::Device>("devices", id)
        .await?
        .context("手机未登记")?
        .display_off;
    let mut connection = tokio::select! {
        result = app.adb.video(id, &server, app.workers.clone(), display_off) => result?,
        _ = app.stop.cancelled() => return Ok(()),
        _ = socket.recv() => return Ok(()),
    };
    let mut state = Control::default();
    let result = relay(app, id, socket, &mut connection, &mut state).await;
    if let Some(control) = connection.control.as_mut() {
        let _ = state.release(control).await;
    }
    connection.stop().await;
    free(app, id).await;
    result
}
async fn relay(
    app: &App,
    id: &str,
    socket: &mut WebSocket,
    connection: &mut crate::adb::VideoConnection,
    state: &mut Control,
) -> Result<()> {
    let stream = connection.stream.as_mut().context("投屏通道未连接")?;
    let control = connection.control.as_mut().context("操作通道未连接")?;
    let errors = connection.errors.as_mut().context("投屏状态未连接")?;
    let mut logs_open = true;
    let mut buffer = [0u8; 64 * 1024];
    let mut device_messages = [0u8; 4096];
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    let mut last_seen = tokio::time::Instant::now();
    let mut rate_window = tokio::time::Instant::now();
    let mut rate = 0u32;
    let mut disabled = false;
    let display_off = app
        .db
        .get::<crate::types::Device>("devices", id)
        .await?
        .context("手机未登记")?
        .display_off;
    tokio::time::timeout(
        Duration::from_secs(2),
        socket.send(Message::Text(
            json!({"type":"display_power","enabled":!display_off,"initial":true})
                .to_string()
                .into(),
        )),
    )
    .await??;
    loop {
        tokio::select! {
            _ = app.stop.cancelled() => return Ok(()),
            error = errors.recv(), if logs_open => match error {
                Some(error) => {
                    disabled = true;
                    state.release(control).await?;
                    free(app,id).await;
                    control_status(socket,false,Some(error)).await?;
                },
                None => logs_open = false,
            },
            count = control.read(&mut device_messages) => {
                if count.context("手机操作连接中断")? == 0 { bail!("手机操作连接已结束，请重新投屏") }
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Pong(_))) => last_seen = tokio::time::Instant::now(),
                Some(Ok(Message::Close(_))) | None => return Ok(()),
                Some(Err(error)) => return Err(error.into()),
                Some(Ok(Message::Text(text))) => {
                    ensure!(text.len() <= 256, "操作消息过长");
                    match serde_json::from_str::<Request>(&text).context("操作请求无效")? {
                        Request::Control {enabled:true} => {
                            let result = if disabled { Err(anyhow::anyhow!("手机拒绝操作，请检查调试权限后重新投屏")) } else { acquire(app,id).await };
                            match result {
                                Ok(()) => { state.enabled = true; control_status(socket,true,None).await?; },
                                Err(error) => { control_status(socket,false,Some(error.to_string())).await?; },
                            }
                        },
                        Request::Control {enabled:false} => {
                            state.release(control).await?;
                            free(app,id).await;
                            control_status(socket,false,None).await?;
                        },
                    }
                },
                Some(Ok(Message::Binary(data))) => {
                    if disabled { continue; }
                    if rate_window.elapsed() >= Duration::from_secs(1) { rate_window = tokio::time::Instant::now(); rate = 0; }
                    rate += 1;
                    ensure!(rate <= 240, "操作过于频繁，请重新开启操作");
                    if data.len() == 2 {
                        let _gate = app.device_gate.lock().await;
                        state.inject(control,&data).await?;
                        let mut device = app.db.get::<crate::types::Device>("devices", id).await?
                            .context("手机未登记")?;
                        device.display_off = data[1] == 0;
                        app.db.put("devices", id, &device).await?;
                        let _ = app.events.send(json!({"type":"changed"}));
                        tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Text(
                            json!({"type":"display_power","enabled":data[1] == 2}).to_string().into()
                        ))).await??;
                    } else {
                        state.inject(control,&data).await?;
                    }
                },
                Some(Ok(Message::Ping(_))) => {},
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

#[cfg(test)]
mod tests {
    use super::*;
    fn touch(action: u8) -> Vec<u8> {
        let mut data = vec![0; 32];
        data[0] = 2;
        data[1] = action;
        data[18..20].copy_from_slice(&400u16.to_be_bytes());
        data[20..22].copy_from_slice(&800u16.to_be_bytes());
        data
    }
    #[test]
    fn touch_is_single_finger_bounded_and_sequential() {
        let mut state = Control::default();
        assert!(state.validate(&touch(0)).is_err());
        state.enabled = true;
        assert!(state.validate(&touch(0)).is_ok());
        assert!(state.validate(&touch(2)).is_err());
        state.touch = Some(touch(0));
        assert!(state.validate(&touch(0)).is_err());
        for action in [1, 2, 3] {
            assert!(state.validate(&touch(action)).is_ok());
        }
        let mut outside = touch(2);
        outside[10..14].copy_from_slice(&400u32.to_be_bytes());
        assert!(state.validate(&outside).is_err());
        let mut other = touch(2);
        other[9] = 1;
        assert!(state.validate(&other).is_err());
        assert!(state.validate(&touch(4)).is_err());
    }
    #[test]
    fn display_power_requires_control_and_valid_modes() {
        let mut state = Control::default();
        assert!(state.validate(&[10, 0]).is_err());
        state.enabled = true;
        for mode in [0, 2] {
            assert!(state.validate(&[10, mode]).is_ok());
        }
        assert!(state.validate(&[10, 1]).is_err());
        assert!(state.validate(&[11, 0]).is_err());
        state.touch = Some(touch(0));
        assert!(state.validate(&[10, 0]).is_err());
    }
    #[test]
    fn keys_are_paired_and_limited_to_navigation() {
        let state = Control {
            enabled: true,
            ..Default::default()
        };
        for key in [3u32, 4, 187, 26] {
            let mut data = vec![0; 28];
            data[15] = 1;
            data[2..6].copy_from_slice(&key.to_be_bytes());
            data[16..20].copy_from_slice(&key.to_be_bytes());
            assert_eq!(state.validate(&data).is_ok(), key != 26);
            data[15] = 0;
            assert!(state.validate(&data).is_err());
        }
    }
}
