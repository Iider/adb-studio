use crate::{App, types::*};
use anyhow::{Context, Result, bail};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::process::Command;

pub struct Adb {
    path: PathBuf,
    targets: tokio::sync::RwLock<std::collections::HashMap<String, Vec<String>>>,
}
impl Adb {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            targets: tokio::sync::RwLock::new(Default::default()),
        }
    }
    pub async fn raw(&self, args: &[&str]) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.path);
        cmd.args(args).kill_on_drop(true);
        let out = tokio::time::timeout(Duration::from_secs(12), cmd.output())
            .await
            .context("ADB 响应超时")??;
        if !out.status.success() {
            bail!("ADB：{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(out.stdout)
    }
    pub async fn text(&self, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8(self.raw(args).await?)?.trim().to_owned())
    }
    pub async fn endpoint(&self, id: &str) -> Result<String> {
        let listing = self.text(&["devices", "-l"]).await?;
        let hints = self
            .targets
            .read()
            .await
            .get(id)
            .cloned()
            .unwrap_or_default();
        let mut endpoints = listing
            .lines()
            .skip(1)
            .filter_map(|line| {
                let p: Vec<_> = line.split_whitespace().collect();
                (p.len() > 1 && p[1] == "device").then(|| p[0].to_owned())
            })
            .collect::<Vec<_>>();
        endpoints.sort_by_key(|endpoint| {
            if endpoint == id {
                0
            } else if hints.contains(endpoint) {
                1
            } else {
                2
            }
        });
        for endpoint in endpoints {
            let Ok(actual) = self
                .text(&["-s", &endpoint, "shell", "getprop", "ro.serialno"])
                .await
            else {
                continue;
            };
            if actual == id {
                self.remember(id, &endpoint).await;
                return Ok(endpoint);
            }
        }
        bail!("手机未连接或未授权")
    }
    async fn remember(&self, id: &str, endpoint: &str) {
        let mut targets = self.targets.write().await;
        let hints = targets.entry(id.into()).or_default();
        if !hints.iter().any(|value| value == endpoint) {
            hints.push(endpoint.into());
            if hints.len() > 8 {
                hints.remove(0);
            }
        }
    }
    pub async fn phone(&self, id: &str, args: &[&str]) -> Result<Vec<u8>> {
        let endpoint = self.endpoint(id).await?;
        let mut all = vec!["-s", endpoint.as_str()];
        all.extend_from_slice(args);
        self.raw(&all).await
    }
    pub async fn phone_text(&self, id: &str, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8(self.phone(id, args).await?)?
            .trim()
            .into())
    }
    pub async fn shot(&self, id: &str) -> Result<Vec<u8>> {
        let displays = self
            .phone_text(id, &["shell", "dumpsys", "display"])
            .await?;
        let display = active_capture_display(&displays)?;
        let mut args = vec!["exec-out", "screencap", "-p"];
        if let Some(display) = display.as_deref() {
            args.extend(["-d", display]);
        }
        clean_screenshot(self.phone(id, &args).await?)
    }
    pub async fn screen_control(&self, id: &str, wake: bool) -> Result<String> {
        self.phone(
            id,
            &[
                "shell",
                "input",
                "keyevent",
                if wake {
                    "KEYCODE_WAKEUP"
                } else {
                    "KEYCODE_SLEEP"
                },
            ],
        )
        .await?;
        if wake {
            self.phone(id, &["shell", "wm", "dismiss-keyguard"]).await?;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
        let policy = self
            .phone_text(id, &["shell", "dumpsys", "window", "policy"])
            .await?;
        if wake {
            Ok(if keyguard_locked(&policy) {
                "屏幕已唤醒；需要在手机上输入密码或使用指纹解锁"
            } else {
                "已唤醒，锁屏已解除"
            }
            .into())
        } else {
            let power = self.phone_text(id, &["shell", "dumpsys", "power"]).await?;
            Ok(
                if power.contains("mWakefulness=Asleep") || power.contains("mInteractive=false") {
                    "屏幕已关闭"
                } else {
                    "已发送锁屏指令，请查看手机"
                }
                .into(),
            )
        }
    }
    pub async fn foreground(&self, id: &str) -> Result<String> {
        let s = self
            .phone_text(id, &["shell", "dumpsys", "activity", "activities"])
            .await?;
        Ok(s.lines()
            .filter(|l| l.contains("topResumedActivity") || l.contains("mResumedActivity"))
            .collect::<Vec<_>>()
            .join(" "))
    }
    pub async fn tap(&self, id: &str, x: u32, y: u32) -> Result<()> {
        self.phone(
            id,
            &["shell", "input", "tap", &x.to_string(), &y.to_string()],
        )
        .await?;
        Ok(())
    }
    pub async fn swipe(&self, id: &str, from: [f64; 2], to: [f64; 2], ms: u32) -> Result<()> {
        let (w, h) = self.size(id).await?;
        let p = |v: f64, n: u32| ((v * n as f64).round() as u32).min(n - 1).to_string();
        self.phone(
            id,
            &[
                "shell",
                "input",
                "swipe",
                &p(from[0], w),
                &p(from[1], h),
                &p(to[0], w),
                &p(to[1], h),
                &ms.to_string(),
            ],
        )
        .await?;
        Ok(())
    }
    pub async fn size(&self, id: &str) -> Result<(u32, u32)> {
        let png = self.shot(id).await?;
        screen_size(&png)
    }
}

fn keyguard_locked(policy: &str) -> bool {
    policy.contains("mShowingLockscreen=true")
        || policy
            .split("KeyguardServiceDelegate")
            .nth(1)
            .is_some_and(|value| {
                value.contains("showing=true") || value.contains("mIsShowing=true")
            })
}

fn active_capture_display(dump: &str) -> Result<Option<String>> {
    let displays: Vec<_> = dump
        .lines()
        .filter(|line| {
            line.trim_start().starts_with("DisplayDeviceInfo{") && line.contains("type INTERNAL")
        })
        .filter_map(|line| {
            let id = line.split("uniqueId=\"local:").nth(1)?.split('"').next()?;
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            Some((id, line.contains("state ON,")))
        })
        .collect();
    if displays.len() <= 1 {
        return Ok(None);
    }
    let active: Vec<_> = displays.iter().filter(|(_, on)| *on).collect();
    if active.len() == 1 {
        return Ok(Some(active[0].0.to_owned()));
    }
    bail!("无法确认当前亮着的屏幕，请点亮目标屏幕后重新截图")
}

fn clean_screenshot(mut output: Vec<u8>) -> Result<Vec<u8>> {
    // 部分多屏手机会将显示屏提示写到 PNG 前面的标准输出中。
    let start = output
        .windows(8)
        .position(|bytes| bytes == b"\x89PNG\r\n\x1a\n")
        .context("手机未返回有效截图，请解锁后重试")?;
    output.drain(..start);
    image::load_from_memory_with_format(&output, image::ImageFormat::Png)
        .context("手机截图不完整，请重新截图")?;
    Ok(output)
}

fn screen_size(png: &[u8]) -> Result<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        bail!("截图无法读取当前屏幕尺寸")
    }
    let width = u32::from_be_bytes(png[16..20].try_into()?);
    let height = u32::from_be_bytes(png[20..24].try_into()?);
    if width == 0 || height == 0 {
        bail!("屏幕尺寸无效")
    }
    Ok((width, height))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folded_and_unfolded_capture_the_lit_display() {
        let folded = "DisplayDeviceInfo{uniqueId=\"local:100\", type INTERNAL, state OFF,}\nDisplayDeviceInfo{uniqueId=\"local:200\", type INTERNAL, state ON,}";
        assert_eq!(
            active_capture_display(folded).unwrap().as_deref(),
            Some("200")
        );
        let unfolded = folded
            .replace("state OFF", "state TEMP")
            .replace("state ON", "state OFF")
            .replace("state TEMP", "state ON");
        assert_eq!(
            active_capture_display(&unfolded).unwrap().as_deref(),
            Some("100")
        );
        assert!(active_capture_display(&folded.replace("state ON", "state OFF")).is_err());
        assert_eq!(
            active_capture_display(
                "DisplayDeviceInfo{uniqueId=\"local:100\", type INTERNAL, state ON,}"
            )
            .unwrap(),
            None
        );
    }
    #[test]
    fn screenshot_accepts_display_warning_and_rejects_invalid_data() {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let png = encoded.into_inner();
        assert_eq!(clean_screenshot(png.clone()).unwrap(), png);
        let mut warned = b"[Warning] Multiple displays were found\n".to_vec();
        warned.extend_from_slice(&png);
        assert_eq!(clean_screenshot(warned).unwrap(), png);
        assert!(clean_screenshot(b"error".to_vec()).is_err());
        assert!(clean_screenshot(png[..24].to_vec()).is_err());
    }
    #[test]
    fn landscape_uses_current_capture_dimensions() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend([0u8; 8]);
        png.extend(2400u32.to_be_bytes());
        png.extend(1080u32.to_be_bytes());
        assert_eq!(screen_size(&png).unwrap(), (2400, 1080));
        assert!(screen_size(b"not png").is_err());
    }
}

pub async fn refresh(app: &App) -> Result<()> {
    let _gate = app.device_gate.lock().await;
    let mut devices = app.db.list::<Device>("devices").await?;
    for d in &mut devices {
        d.online = false;
        d.endpoints.clear();
    }
    let listing = app.adb.text(&["devices", "-l"]).await?;
    for line in listing.lines().skip(1) {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() < 2 || parts[1] != "device" {
            continue;
        }
        let endpoint = parts[0];
        let Ok(id) = app
            .adb
            .text(&["-s", endpoint, "shell", "getprop", "ro.serialno"])
            .await
        else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        app.adb.remember(&id, endpoint).await;
        let model = app
            .adb
            .text(&["-s", endpoint, "shell", "getprop", "ro.product.model"])
            .await
            .unwrap_or_default();
        let index = devices.iter().position(|d| d.id == id).unwrap_or_else(|| {
            devices.push(Device::new(&id, &model, &model));
            devices.len() - 1
        });
        let d = &mut devices[index];
        d.online = true;
        d.endpoints.push(endpoint.into());
        d.last_seen = Some(now());
        d.model = model;
        d.connection = if endpoint.contains(':') {
            "无线".into()
        } else {
            "USB".into()
        };
        let lock = app
            .adb
            .text(&["-s", endpoint, "shell", "dumpsys", "window", "policy"])
            .await
            .unwrap_or_default();
        d.locked = keyguard_locked(&lock);
    }
    for mut d in devices {
        if d.online {
            if let Err(e) = apply_awake(app, &mut d).await {
                d.note = e.to_string();
            }
        }
        app.db.put("devices", &d.id, &d).await?;
    }
    let _ = app.events.send(serde_json::json!({"type":"changed"}));
    Ok(())
}

pub async fn apply_awake(app: &App, d: &mut Device) -> Result<()> {
    if d.keep_awake && !d.awake_applied {
        if d.original_awake.is_none() {
            d.original_awake = Some(
                app.adb
                    .phone_text(
                        &d.id,
                        &[
                            "shell",
                            "settings",
                            "get",
                            "global",
                            "stay_on_while_plugged_in",
                        ],
                    )
                    .await?,
            );
            d.original_timeout = Some(
                app.adb
                    .phone_text(
                        &d.id,
                        &["shell", "settings", "get", "system", "screen_off_timeout"],
                    )
                    .await?,
            );
            app.db.put("devices", &d.id, d).await?;
        }
        app.adb
            .phone(
                &d.id,
                &[
                    "shell",
                    "settings",
                    "put",
                    "global",
                    "stay_on_while_plugged_in",
                    "7",
                ],
            )
            .await?;
        app.adb
            .phone(
                &d.id,
                &[
                    "shell",
                    "settings",
                    "put",
                    "system",
                    "screen_off_timeout",
                    "2147483647",
                ],
            )
            .await?;
        d.awake_applied = true;
        d.note = "已开启常亮，建议保持接电".into();
    } else if !d.keep_awake && (d.awake_applied || d.restore_pending || d.original_awake.is_some())
    {
        for (scope, key, value) in [
            (
                "global",
                "stay_on_while_plugged_in",
                d.original_awake.as_deref(),
            ),
            (
                "system",
                "screen_off_timeout",
                d.original_timeout.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                if value == "null" {
                    app.adb
                        .phone(&d.id, &["shell", "settings", "delete", scope, key])
                        .await?;
                } else {
                    app.adb
                        .phone(&d.id, &["shell", "settings", "put", scope, key, value])
                        .await?;
                }
            }
        }
        d.original_awake = None;
        d.original_timeout = None;
        d.awake_applied = false;
        d.restore_pending = false;
        d.note = "已恢复原来的息屏设置".into();
    }
    if d.keep_awake && !d.locked {
        app.adb
            .phone(&d.id, &["shell", "input", "keyevent", "KEYCODE_WAKEUP"])
            .await?;
    }
    Ok(())
}

pub async fn monitor(app: Arc<App>) {
    let mut ticks = 0u32;
    loop {
        if let Err(e) = refresh(&app).await {
            tracing::warn!("设备检查：{e}");
        }
        ticks += 1;
        if ticks.is_multiple_of(5) {
            if let (Ok(services), Ok(devices)) = (
                app.adb.text(&["mdns", "services"]).await,
                app.db.list::<Device>("devices").await,
            ) {
                for line in services
                    .lines()
                    .filter(|l| l.contains("_adb-tls-connect._tcp"))
                {
                    if devices.iter().any(|d| !d.online && line.contains(&d.id)) {
                        if let Some(address) = line.split_whitespace().last() {
                            let _ = app.adb.text(&["connect", address]).await;
                        }
                    }
                }
            }
        }
        tokio::select! {_=app.stop.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs(3))=>{}}
    }
}
