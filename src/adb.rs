use crate::{App, types::*};
use anyhow::{Context, Result, bail};
use image::{
    ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use serde::Serialize;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum AdbIssue {
    TimedOut {
        command: String,
        selector: String,
        elapsed_ms: u64,
    },
    InputDenied(String),
}
impl AdbIssue {
    pub fn summary(&self) -> &'static str {
        match self {
            Self::TimedOut { .. } => {
                "ADB 响应超时，已暂停。请检查手机连接与其他 ADB 操作，再点击继续。"
            }
            Self::InputDenied(_) => {
                "手机拒绝触控，已暂停。请检查手机状态、调试权限或操作冲突，再点击继续。"
            }
        }
    }
}
impl std::fmt::Display for AdbIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut {
                command,
                selector,
                elapsed_ms,
            } => write!(
                f,
                "ADB 响应超时：{command}；连接：{selector}；耗时 {:.1} 秒（时限 12 秒）",
                *elapsed_ms as f64 / 1000.0
            ),
            Self::InputDenied(detail) => write!(f, "ADB：{detail}"),
        }
    }
}
impl std::error::Error for AdbIssue {}

fn command_diagnostic(args: &[&str]) -> (String, String) {
    let (selector, command) = if args.len() >= 2 && matches!(args[0], "-s" | "-t") {
        (format!("{} {}", args[0], args[1]), &args[2..])
    } else {
        ("ADB 服务".into(), args)
    };
    let safe = if command.first() == Some(&"pair") {
        format!("pair {} [配对码已隐藏]", command.get(1).unwrap_or(&""))
    } else if command.starts_with(&["shell", "input"]) && command.contains(&"text") {
        "shell input text [内容已隐藏]".into()
    } else {
        command.join(" ")
    };
    let clean = |value: String| {
        value
            .chars()
            .take(512)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect()
    };
    (clean(selector), clean(safe))
}

fn connection_kind(endpoint: &str, metadata: &[&str]) -> &'static str {
    let endpoint = endpoint.trim_end_matches('.');
    if metadata.iter().any(|part| part.starts_with("usb:")) {
        "USB"
    } else if endpoint.contains(':')
        || endpoint.ends_with("._adb-tls-connect._tcp")
        || endpoint.ends_with("._adb._tcp")
    {
        "无线"
    } else {
        "USB"
    }
}

pub struct Screenshot {
    pub png: Vec<u8>,
    pub image: image::DynamicImage,
    pub timing: ScreenshotTiming,
}

#[derive(Default)]
pub struct ScreenshotTiming {
    device_ms: f64,
    display_ms: f64,
    capture_ms: f64,
    decode_ms: f64,
    encode_ms: f64,
    total_ms: f64,
    source: &'static str,
}
impl ScreenshotTiming {
    pub fn header(&self) -> String {
        format!(
            "device;dur={:.1}, display;dur={:.1}, capture;dur={:.1}, decode;dur={:.1}, encode;dur={:.1}, total;dur={:.1}, source;desc=\"{}\"",
            self.device_ms,
            self.display_ms,
            self.capture_ms,
            self.decode_ms,
            self.encode_ms,
            self.total_ms,
            self.source
        )
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[derive(Clone)]
struct Target {
    endpoint: String,
    transport: Option<String>,
    usb: bool,
}
impl Target {
    fn selector(&self) -> [&str; 2] {
        match self.transport.as_deref() {
            Some(transport) => ["-t", transport],
            None => ["-s", &self.endpoint],
        }
    }
}

pub struct Adb {
    path: PathBuf,
    targets: tokio::sync::RwLock<std::collections::HashMap<String, Vec<String>>>,
    server_gate: tokio::sync::RwLock<()>,
}
impl Adb {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            targets: tokio::sync::RwLock::new(Default::default()),
            server_gate: tokio::sync::RwLock::new(()),
        }
    }
    pub async fn raw(&self, args: &[&str]) -> Result<Vec<u8>> {
        let _gate = self.server_gate.read().await;
        self.raw_unlocked(args).await
    }
    async fn restart(&self) -> Result<()> {
        let _gate = self.server_gate.write().await;
        self.raw_unlocked(&["kill-server"])
            .await
            .context("停止 ADB 失败")?;
        self.raw_unlocked(&["start-server"])
            .await
            .context("启动 ADB 失败，请检查电脑上的 ADB 服务")?;
        self.targets.write().await.clear();
        Ok(())
    }
    async fn raw_unlocked(&self, args: &[&str]) -> Result<Vec<u8>> {
        let mut cmd = crate::platform::command(&self.path);
        // macOS 后台服务使用原生 USB 驱动时可能卡在 IOServiceOpen。
        #[cfg(target_os = "macos")]
        if std::env::var_os("ADB_LIBUSB").is_none() {
            cmd.env("ADB_LIBUSB", "1");
        }
        cmd.args(args).kill_on_drop(true);
        let started = Instant::now();
        let out = tokio::time::timeout(Duration::from_secs(12), cmd.output())
            .await
            .map_err(|_| {
                let elapsed_ms = started.elapsed().as_millis() as u64;
                let (selector, command) = command_diagnostic(args);
                tracing::warn!(%command, adb_target = %selector, elapsed_ms, timeout_ms = 12000, "ADB 命令超时");
                AdbIssue::TimedOut { command, selector, elapsed_ms }
            })??;
        if !out.status.success() {
            let detail = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            if detail.contains("INJECT_EVENTS") {
                return Err(AdbIssue::InputDenied(detail).into());
            }
            bail!("ADB：{detail}");
        }
        Ok(out.stdout)
    }
    pub async fn text(&self, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8(self.raw(args).await?)?.trim().to_owned())
    }
    pub async fn endpoint(&self, id: &str) -> Result<String> {
        Ok(self.target(id).await?.endpoint)
    }
    #[tracing::instrument(skip_all, fields(device_id = id))]
    async fn target(&self, id: &str) -> Result<Target> {
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
                (p.len() > 1 && p[1] == "device").then(|| Target {
                    endpoint: p[0].to_owned(),
                    usb: p.iter().any(|part| part.starts_with("usb:")),
                    transport: p.iter().find_map(|part| {
                        part.strip_prefix("transport_id:")
                            .filter(|value| {
                                !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
                            })
                            .map(str::to_owned)
                    }),
                })
            })
            .collect::<Vec<_>>();
        endpoints.sort_by_key(|target| {
            if target.endpoint == id {
                0
            } else if hints.contains(&target.endpoint) {
                1
            } else {
                2
            }
        });
        for target in endpoints {
            let selector = target.selector();
            let Ok(actual) = self
                .text(&[selector[0], selector[1], "shell", "getprop", "ro.serialno"])
                .await
            else {
                continue;
            };
            if actual == id {
                self.remember(id, &target.endpoint).await;
                return Ok(target);
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
    #[tracing::instrument(skip_all, fields(device_id = id))]
    pub async fn phone(&self, id: &str, args: &[&str]) -> Result<Vec<u8>> {
        let endpoint = self.endpoint(id).await?;
        let mut all = vec!["-s", endpoint.as_str()];
        all.extend_from_slice(args);
        self.raw(&all).await
    }
    async fn at_target(&self, target: &Target, args: &[&str]) -> Result<Vec<u8>> {
        let mut all = target.selector().to_vec();
        all.extend_from_slice(args);
        self.raw(&all).await
    }
    pub async fn phone_text(&self, id: &str, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8(self.phone(id, args).await?)?
            .trim()
            .into())
    }
    #[tracing::instrument(skip_all, fields(device_id = id))]
    pub async fn video(
        &self,
        id: &str,
        server: &std::path::Path,
        cleanup: tokio_util::task::TaskTracker,
        display_off: bool,
    ) -> Result<VideoConnection> {
        if !server.is_file() {
            bail!("投屏组件尚未安装，请安装投屏组件并更新服务（见实时投屏说明）")
        }
        let target = self.target(id).await?;
        let power = self
            .at_target(&target, &["shell", "dumpsys", "power"])
            .await?;
        if !display_off && !String::from_utf8_lossy(&power).contains("mWakefulness=Awake") {
            bail!("手机已息屏，请先唤醒并解锁，再开启投屏")
        }
        let scid = format!(
            "{:08x}",
            (uuid::Uuid::new_v4().as_u128() as u32) & 0x7fff_ffff
        );
        let remote = format!("/data/local/tmp/adb-studio-{scid}.jar");
        let socket = format!("localabstract:scrcpy_{scid}");
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = format!("tcp:{}", listener.local_addr()?.port());
        let mut connection = VideoConnection {
            child: None,
            cleanup,
            stream: None,
            control: None,
            errors: None,
            path: self.path.clone(),
            target,
            socket,
            remote: remote.clone(),
        };
        self.at_target(
            &connection.target,
            &[
                "push",
                server.to_str().context("投屏组件路径无效")?,
                &remote,
            ],
        )
        .await?;
        self.at_target(&connection.target, &["reverse", &connection.socket, &port])
            .await?;
        let classpath = format!("CLASSPATH={remote}");
        let session = format!("scid={scid}");
        let child = crate::platform::command(&self.path)
            .args(connection.target.selector())
            .args([
                "shell",
                &classpath,
                "app_process",
                "/",
                "com.genymobile.scrcpy.Server",
                "3.1",
                &session,
                "tunnel_forward=false",
                "audio=false",
                "control=true",
                "clipboard_autosync=false",
                "power_on=false",
                // scrcpy 的退出清理会把控制指令关闭的实体屏幕重新点亮。
                // 本工作台自行清理 reverse 通道与临时文件。
                "cleanup=false",
                "video_codec=h264",
                "max_size=1280",
                "max_fps=30",
                "video_bit_rate=4000000",
                "send_dummy_byte=false",
                "send_device_meta=false",
                "send_codec_meta=true",
                "send_frame_meta=true",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("无法启动手机投屏")?;
        connection.child = Some(child);
        let (error_tx, error_rx) = tokio::sync::mpsc::channel(4);
        let child = connection.child.as_mut().unwrap();
        for pipe in [
            child
                .stdout
                .take()
                .map(|p| Box::pin(p) as std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>),
            child
                .stderr
                .take()
                .map(|p| Box::pin(p) as std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>),
        ]
        .into_iter()
        .flatten()
        {
            let tx = error_tx.clone();
            connection.cleanup.spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let mut lines = tokio::io::BufReader::new(pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.contains("INJECT_EVENTS") || line.contains("Could not inject") {
                        let _ = tx.try_send(
                            "手机拒绝操作，请检查调试权限或其他控制软件，再重新投屏".to_owned(),
                        );
                    }
                }
            });
        }
        connection.errors = Some(error_rx);
        tokio::time::timeout(Duration::from_secs(10), async {
            connection.stream = Some(listener.accept().await?.0);
            connection.control = Some(listener.accept().await?.0);
            Ok::<_, std::io::Error>(())
        })
        .await
        .context("投屏连接超时，请解锁手机并检查连接后重试")??;
        Ok(connection)
    }
    pub async fn shot(&self, id: &str) -> Result<Vec<u8>> {
        Ok(self.capture(id).await?.png)
    }
    #[tracing::instrument(skip_all, fields(device_id = id))]
    pub async fn capture(&self, id: &str) -> Result<Screenshot> {
        let started = Instant::now();
        let mut timing = ScreenshotTiming::default();
        let phase = Instant::now();
        let mut target = self.target(id).await?;
        timing.device_ms += elapsed_ms(phase);
        let phase = Instant::now();
        let displays = String::from_utf8(
            self.at_target(&target, &["shell", "dumpsys", "display"])
                .await?,
        )?;
        let display = active_capture_display(&displays)?;
        timing.display_ms = elapsed_ms(phase);
        let mut args = vec!["exec-out", "screencap"];
        if let Some(display) = display.as_deref() {
            args.extend(["-d", display]);
        }
        // transport_id 固定本次已核验连接；旧版 ADB 没有该编号时，保留逐步核验。
        if target.transport.is_none() {
            let phase = Instant::now();
            target = self.target(id).await?;
            timing.device_ms += elapsed_ms(phase);
        }
        let raw_image = if target.usb {
            let phase = Instant::now();
            let output = self.at_target(&target, &args).await;
            timing.capture_ms = elapsed_ms(phase);
            let phase = Instant::now();
            let result = output.and_then(raw_screenshot);
            timing.decode_ms = elapsed_ms(phase);
            match result {
                Ok(image) => Some(image),
                Err(error) => {
                    tracing::debug!(%id, %error, "原始画面不可用，回退手机 PNG");
                    None
                }
            }
        } else {
            None
        };
        let (png, image) = match raw_image {
            Some(image) => {
                let phase = Instant::now();
                let mut png = Vec::new();
                PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Sub)
                    .write_image(
                        image.as_bytes(),
                        image.width(),
                        image.height(),
                        image.color().into(),
                    )?;
                timing.encode_ms = elapsed_ms(phase);
                timing.source = "raw";
                (png, image)
            }
            None => {
                if target.usb && target.transport.is_none() {
                    let phase = Instant::now();
                    target = self.target(id).await?;
                    timing.device_ms += elapsed_ms(phase);
                }
                args.insert(2, "-p");
                let phase = Instant::now();
                let output = self.at_target(&target, &args).await?;
                timing.capture_ms += elapsed_ms(phase);
                let phase = Instant::now();
                let result = clean_screenshot(output)?;
                timing.decode_ms += elapsed_ms(phase);
                timing.source = "png";
                result
            }
        };
        timing.total_ms = elapsed_ms(started);
        Ok(Screenshot { png, image, timing })
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
    pub async fn check_unlocked(&self, id: &str) -> Result<()> {
        self.check_unlocked_for_mirror(id, false).await
    }
    pub async fn check_unlocked_for_mirror(&self, id: &str, display_off: bool) -> Result<()> {
        let policy = self
            .phone_text(id, &["shell", "dumpsys", "window", "policy"])
            .await?;
        if keyguard_locked(&policy) {
            bail!("请先在手机上解锁，再点击继续")
        }
        let power = self.phone_text(id, &["shell", "dumpsys", "power"]).await?;
        if !display_off
            && (power.contains("mWakefulness=Asleep") || power.contains("mInteractive=false"))
        {
            bail!("请先唤醒手机并回到目标页面，再点击继续")
        }
        Ok(())
    }
    pub async fn size(&self, id: &str) -> Result<(u32, u32)> {
        let png = self.shot(id).await?;
        screen_size(&png)
    }

    #[tracing::instrument(skip_all, fields(device_id = id))]
    pub async fn canvas_screen(&self, id: &str) -> Result<ScreenGeometry> {
        let dump = String::from_utf8(self.phone(id, &["shell", "dumpsys", "display"]).await?)?;
        let physical = active_capture_display(&dump)?;
        let logical = physical
            .as_deref()
            .map(|physical| logical_input_display(&dump, physical))
            .transpose()?;
        let (width, height) = self.size(id).await?;
        let after = String::from_utf8(self.phone(id, &["shell", "dumpsys", "display"]).await?)?;
        anyhow::ensure!(
            active_capture_display(&after)? == physical,
            "屏幕形态已改变，请重新标注"
        );
        Ok(ScreenGeometry {
            width,
            height,
            display: physical.unwrap_or_else(|| "default".into()),
            input_display: logical.unwrap_or(0),
        })
    }
}

#[derive(Clone, serde::Serialize)]
pub struct ScreenGeometry {
    pub width: u32,
    pub height: u32,
    pub display: String,
    pub input_display: u32,
}

pub struct VideoConnection {
    cleanup: tokio_util::task::TaskTracker,
    child: Option<tokio::process::Child>,
    pub stream: Option<tokio::net::TcpStream>,
    pub control: Option<tokio::net::TcpStream>,
    pub errors: Option<tokio::sync::mpsc::Receiver<String>>,
    path: PathBuf,
    target: Target,
    socket: String,
    remote: String,
}
impl VideoConnection {
    pub async fn stop(&mut self) {
        if let Some(mut control) = self.control.take() {
            use tokio::io::AsyncWriteExt;
            let _ = control.shutdown().await;
        }
        if let Some(mut child) = self.child.take() {
            // 控制通道 EOF 排在取消触点之后；先让手机处理完，再等待服务退出。
            if tokio::time::timeout(Duration::from_secs(1), child.wait())
                .await
                .is_err()
            {
                let _ = tokio::time::timeout(Duration::from_secs(3), child.kill()).await;
            }
        }
        self.stream.take();
    }
}
impl Drop for VideoConnection {
    fn drop(&mut self) {
        self.stream.take();
        self.control.take();
        self.child.take();
        let path = self.path.clone();
        let target = self.target.clone();
        let socket = self.socket.clone();
        let remote = self.remote.clone();
        self.cleanup.spawn(async move {
            let mut command = crate::platform::command(&path);
            command
                .args(target.selector())
                .args(["reverse", "--remove", &socket])
                .kill_on_drop(true);
            let _ = tokio::time::timeout(Duration::from_secs(3), command.output()).await;
            let mut remove = crate::platform::command(path);
            remove
                .args(target.selector())
                .args(["shell", "rm", "-f", &remote])
                .kill_on_drop(true);
            let _ = tokio::time::timeout(Duration::from_secs(3), remove.output()).await;
        });
    }
}

fn raw_screenshot(mut output: Vec<u8>) -> Result<image::DynamicImage> {
    while output.starts_with(b"[Warning]") {
        let end = output
            .iter()
            .position(|&b| b == b'\n')
            .context("截图提示不完整")?;
        output.drain(..=end);
    }
    if output.len() < 16 {
        bail!("原始截图头不完整")
    }
    let word = |offset| u32::from_le_bytes(output[offset..offset + 4].try_into().unwrap());
    let (width, height, format) = (word(0), word(4), word(8));
    if width == 0 || height == 0 || !matches!(format, 1 | 2) {
        bail!("原始截图尺寸或像素格式不支持")
    }
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .context("原始截图尺寸过大")?;
    if output.len().checked_sub(16) != Some(bytes) {
        bail!("原始截图长度不匹配")
    }
    // 仅接收带 sRGB 色彩空间编号的格式；其他格式由手机生成 PNG，避免误读或丢失颜色信息。
    if word(12) != 1 {
        bail!("原始截图色彩空间需要手机 PNG")
    }
    output.drain(..16);
    if format == 2 {
        for pixel in output.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
    }
    Ok(image::DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(width, height, output).context("原始截图像素不完整")?,
    ))
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

fn logical_input_display(dump: &str, physical: &str) -> Result<u32> {
    let mut current = None;
    let mut matches = std::collections::HashSet::new();
    for line in dump.lines() {
        if let Some(value) = line.trim().strip_prefix("mDisplayId=") {
            current = value.trim().parse::<u32>().ok();
        }
        if line.contains("DisplayInfo{") {
            let logical = current.take();
            let same_screen = line
                .split("local:")
                .skip(1)
                .any(|part| part.split(|c: char| !c.is_ascii_digit()).next() == Some(physical));
            if same_screen && let Some(logical) = logical {
                matches.insert(logical);
            }
        }
    }
    anyhow::ensure!(
        matches.len() == 1,
        "无法确认当前屏幕的触控编号，需先适配这台多屏手机"
    );
    Ok(*matches.iter().next().unwrap())
}

fn clean_screenshot(mut output: Vec<u8>) -> Result<(Vec<u8>, image::DynamicImage)> {
    // 部分多屏手机会将显示屏提示写到 PNG 前面的标准输出中。
    let start = output
        .windows(8)
        .position(|bytes| bytes == b"\x89PNG\r\n\x1a\n")
        .context("手机未返回有效截图，请解锁后重试")?;
    output.drain(..start);
    let image = image::load_from_memory_with_format(&output, image::ImageFormat::Png)
        .context("手机截图不完整，请重新截图")?;
    Ok((output, image))
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
    fn connection_kind_handles_mdns_ip_and_usb_metadata() {
        for endpoint in [
            "192.168.1.20:37123",
            "[fd00::2]:5555",
            "adb-phone._adb-tls-connect._tcp",
            "adb-phone._adb-tls-connect._tcp.",
            "adb-phone._adb._tcp",
        ] {
            assert_eq!(connection_kind(endpoint, &[]), "无线");
        }
        assert_eq!(connection_kind("phone-serial", &[]), "USB");
        assert_eq!(connection_kind("usb:serial", &["usb:1-1"]), "USB");
    }

    #[test]
    fn timeout_diagnostic_preserves_command_and_hides_sensitive_arguments() {
        let (selector, command) = command_diagnostic(&["-t", "300", "exec-out", "screencap", "-p"]);
        assert_eq!(selector, "-t 300");
        assert_eq!(command, "exec-out screencap -p");
        let error = AdbIssue::TimedOut {
            command,
            selector,
            elapsed_ms: 12042,
        };
        assert!(error.to_string().contains("12.0 秒"));
        assert!(error.to_string().contains("exec-out screencap -p"));
        let (_, pair) = command_diagnostic(&["pair", "192.168.1.20:12345", "654321"]);
        assert!(!pair.contains("654321"));
        let (_, text) = command_diagnostic(&[
            "-s",
            "phone",
            "shell",
            "input",
            "-d",
            "0",
            "text",
            "private-content",
        ]);
        assert!(!text.contains("private-content"));
        assert_eq!(
            command_diagnostic(&["devices", "-l"]),
            ("ADB 服务".into(), "devices -l".into())
        );
    }
    #[test]
    fn touch_display_requires_its_own_explicit_mapping() {
        let dump = "mDisplayId=0\nDisplayInfo{local:100}\nmDisplayId=3\nDisplayInfo{local:200}";
        assert_eq!(logical_input_display(dump, "200").unwrap(), 3);
        assert!(logical_input_display(&dump.replace("mDisplayId=3\n", ""), "200").is_err());
        assert!(logical_input_display(dump, "20").is_err());
        assert!(
            logical_input_display(
                &format!("{dump}\nmDisplayId=4\nDisplayInfo{{local:200}}"),
                "200"
            )
            .is_err()
        );
    }
    fn raw_image(format: u32, color_space: u32) -> Vec<u8> {
        [
            2u32.to_le_bytes().as_slice(),
            1u32.to_le_bytes().as_slice(),
            format.to_le_bytes().as_slice(),
            color_space.to_le_bytes().as_slice(),
            &[255, 0, 0, 255, 0, 128, 255, 64],
        ]
        .concat()
    }
    #[test]
    fn raw_capture_preserves_pixels_and_ignores_display_warning() {
        let original = raw_image(1, 1);
        let image = raw_screenshot(original.clone()).unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(image.as_bytes(), &original[16..]);
        let mut warned = b"[Warning] Multiple displays were found\n".to_vec();
        warned.extend(original);
        assert_eq!(raw_screenshot(warned).unwrap().as_bytes(), image.as_bytes());
        let rgbx = raw_screenshot(raw_image(2, 1)).unwrap();
        assert_eq!(rgbx.as_bytes(), &[255, 0, 0, 255, 0, 128, 255, 255]);
    }
    #[test]
    fn raw_capture_rejects_incomplete_and_unsupported_images() {
        let raw = raw_image(1, 1);
        for length in 0..raw.len() {
            assert!(raw_screenshot(raw[..length].to_vec()).is_err());
        }
        let mut extra = raw.clone();
        extra.push(0);
        assert!(raw_screenshot(extra).is_err());
        for (format, color) in [(5, 1), (1, 0), (1, 2)] {
            assert!(raw_screenshot(raw_image(format, color)).is_err());
        }
        let mut oversized = raw;
        oversized[..8].fill(255);
        assert!(raw_screenshot(oversized).is_err());
    }
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
        assert_eq!(clean_screenshot(png.clone()).unwrap().0, png);
        let mut warned = b"[Warning] Multiple displays were found\n".to_vec();
        warned.extend_from_slice(&png);
        assert_eq!(clean_screenshot(warned).unwrap().0, png);
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
    #[test]
    fn manual_connection_accepts_only_local_addresses() {
        for value in ["192.168.1.20:37123", "10.0.0.2:5555", "[fd00::2]:5555"] {
            assert!(local_address(value).is_ok(), "{value}");
        }
        for value in [
            "8.8.8.8:5555",
            "example.com:5555",
            "192.168.1.20:0",
            "192.168.1.20",
        ] {
            assert!(local_address(value).is_err(), "{value}");
        }
    }
    #[test]
    fn discovery_lists_local_wireless_debugging_services() {
        let listing = "List of discovered mdns services\nadb-phone._adb-tls-connect._tcp. _adb-tls-connect._tcp. 192.168.1.20:37123\nadb-phone._adb-tls-pairing._tcp. _adb-tls-pairing._tcp. 192.168.1.20:41234\nadb-legacy._adb._tcp. _adb._tcp. 192.168.1.30:5555\nadb-public._adb-tls-connect._tcp. _adb-tls-connect._tcp. 8.8.8.8:5555\n";
        let services = parse_mdns_services(listing);
        assert_eq!(services.len(), 3);
        assert_eq!(services[0].kind, "connect");
        assert_eq!(services[0].address, "192.168.1.20:37123");
        assert_eq!(services[1].kind, "pair");
        assert_eq!(services[2].address, "192.168.1.30:5555");
    }
}

fn local_address(value: &str) -> Result<String> {
    let address: SocketAddr = value
        .trim()
        .parse()
        .context("请输入手机的 IP:端口；IPv6 地址用方括号包住")?;
    let local = match address.ip() {
        std::net::IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_loopback(),
        std::net::IpAddr::V6(ip) => {
            ip.is_unique_local() || ip.is_unicast_link_local() || ip.is_loopback()
        }
    };
    if !local || address.port() == 0 {
        bail!("只支持局域网 IP 和有效端口");
    }
    Ok(address.to_string())
}

#[derive(Serialize, Debug)]
pub struct NearbyDevice {
    pub name: String,
    pub address: String,
    pub kind: String,
}

fn parse_mdns_services(listing: &str) -> Vec<NearbyDevice> {
    let mut devices = Vec::new();
    for line in listing.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let kind =
            if parts[1].contains("_adb-tls-connect._tcp") || parts[1].starts_with("_adb._tcp") {
                "connect"
            } else if parts[1].contains("_adb-tls-pairing._tcp") {
                "pair"
            } else {
                continue;
            };
        let Ok(address) = local_address(parts[parts.len() - 1]) else {
            continue;
        };
        if devices
            .iter()
            .any(|device: &NearbyDevice| device.address == address && device.kind == kind)
        {
            continue;
        }
        devices.push(NearbyDevice {
            name: parts[0].trim_end_matches('.').to_owned(),
            address,
            kind: kind.into(),
        });
    }
    devices.sort_by(|a, b| a.address.cmp(&b.address).then(a.kind.cmp(&b.kind)));
    devices
}

pub async fn discover(app: &App) -> Result<Vec<NearbyDevice>> {
    Ok(parse_mdns_services(
        &app.adb.text(&["mdns", "services"]).await?,
    ))
}

pub async fn pair_manual(app: &App, address: &str, code: &str) -> Result<()> {
    let address = local_address(address)?;
    let code = code.trim();
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("请输入手机显示的 6 位配对码");
    }
    let result = match app.adb.text(&["pair", &address, code]).await {
        Err(error) if error.to_string().contains("protocol fault") => {
            bail!(
                "手机未返回配对回复。请保持手机配对码窗口打开，使用新地址和配对码；确认网络互通，代理绕过局域网。"
            )
        }
        result => result?,
    };
    if !result.contains("Successfully paired") {
        bail!("配对失败，请检查配对地址、端口和配对码");
    }
    Ok(())
}

pub async fn connect_manual(app: &App, address: &str) -> Result<Device> {
    let address = local_address(address)?;
    let result = app
        .adb
        .text(&["connect", &address])
        .await
        .context("无法连接手机，请确认无线调试已开启，并检查连接地址和端口")?;
    if !result.starts_with("connected to ") && !result.starts_with("already connected to ") {
        bail!("连接失败。连接端口可能已变化，请重新查看手机“无线调试”页面的连接地址");
    }
    let id = app
        .adb
        .text(&["-s", &address, "shell", "getprop", "ro.serialno"])
        .await
        .context("无法读取手机编号，请确认无线调试已开启并已完成配对")?;
    if id.is_empty() {
        bail!("手机没有返回编号，请检查无线调试授权");
    }
    refresh(app).await?;
    let _gate = app.device_gate.lock().await;
    let mut device = app
        .db
        .get::<Device>("devices", &id)
        .await?
        .context("手机已连接，但尚未通过设备核验，请刷新后重试")?;
    if !device.online || !device.endpoints.iter().any(|endpoint| endpoint == &address) {
        bail!("手机已连接，但地址未通过设备核验，请刷新后重试");
    }
    device.manual_endpoints = vec![address];
    app.db.put("devices", &id, &device).await?;
    Ok(device)
}

pub async fn restart_server(app: &App) -> Result<Vec<Device>> {
    let _gate = app
        .run_gate
        .try_lock()
        .context("正在处理任务或连接，请稍后再重启 ADB")?;
    if app.db.list::<Run>("runs").await?.iter().any(Run::busy)
        || !app.controls.lock().await.is_empty()
    {
        bail!("有未结束任务，请先停止所有运行、等待手机或已暂停的任务，再重启 ADB");
    }
    app.adb.restart().await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    refresh(app).await?;
    if tokio::time::timeout(Duration::from_secs(15), reconnect_known(app))
        .await
        .is_err()
    {
        tracing::warn!("ADB 已重启，部分手机仍在等待重连");
    }
    refresh(app).await?;
    app.db.list("devices").await
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
        let connection = connection_kind(endpoint, &parts);
        if d.endpoints.len() == 1 || connection == "USB" {
            d.connection = connection.into();
        }
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
    if d.keep_awake && !d.locked && !d.display_off {
        app.adb
            .phone(&d.id, &["shell", "input", "keyevent", "KEYCODE_WAKEUP"])
            .await?;
    }
    Ok(())
}

async fn reconnect_known(app: &App) {
    let Ok(devices) = app.db.list::<Device>("devices").await else {
        return;
    };
    let services = discover(app).await.unwrap_or_default();
    for device in devices.iter().filter(|device| !device.online) {
        if app.adb.endpoint(&device.id).await.is_ok() {
            continue;
        }
        let mut addresses: Vec<_> = services
            .iter()
            .filter(|service| service.kind == "connect" && service.name.contains(&device.id))
            .map(|service| service.address.clone())
            .collect();
        for address in &device.manual_endpoints {
            if !addresses.contains(address) {
                addresses.push(address.clone());
            }
        }
        for address in addresses {
            if devices.iter().any(|other| {
                other.id != device.id && other.online && other.endpoints.contains(&address)
            }) {
                continue;
            }
            let Ok(result) = app.adb.text(&["connect", &address]).await else {
                continue;
            };
            if !result.starts_with("connected to ") && !result.starts_with("already connected to ")
            {
                continue;
            }
            let identity = app
                .adb
                .text(&["-s", &address, "shell", "getprop", "ro.serialno"])
                .await;
            if identity.as_deref().ok() == Some(device.id.as_str()) {
                break;
            }
            let _ = app.adb.text(&["disconnect", &address]).await;
        }
    }
}

pub async fn monitor(app: Arc<App>) {
    let mut ticks = 0u32;
    loop {
        if let Err(e) = refresh(&app).await {
            tracing::warn!("设备检查：{e}");
        }
        ticks += 1;
        if ticks.is_multiple_of(5) {
            reconnect_known(&app).await;
        }
        tokio::select! {_=app.stop.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs(3))=>{}}
    }
}
