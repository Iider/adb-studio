use crate::{
    App,
    recognition::{self, Frame},
    run_control::{PauseRequested, RunControl},
    types::*,
};
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub fn validate(w: &Workflow) -> Result<()> {
    crate::canvas::validate(w)
}

pub fn samples(w: &Workflow) -> Result<()> {
    validate(w)?;
    for sample in &w.tests {
        let node = w
            .nodes
            .iter()
            .find(|node| node.id == sample.node)
            .context("样本步骤不存在")?;
        let Action::Canvas { step } = &node.action else {
            bail!("此步骤不支持样本检查")
        };
        if crate::canvas::sample_choice(step, &sample.text)? != sample.expected {
            bail!("判断样本未通过：{}", node.label)
        }
    }
    for node in &w.nodes {
        if let Action::Canvas { step } = &node.action {
            crate::canvas::require_samples(step, &w.tests)?;
        }
    }
    Ok(())
}

pub async fn workflow(app: &App, id: &str) -> Result<Workflow> {
    app.db
        .get::<Workflow>("workflows", id)
        .await?
        .context("工作流不存在")
}
pub async fn start(app: Arc<App>, device_id: &str, mut w: Workflow) -> Result<Run> {
    let _gate = app.run_gate.lock().await;
    if app.db.get::<Workflow>("workflows", &w.id).await?.is_none() {
        bail!("任务已移除，请重新创建")
    }
    validate(&w)?;
    let needs_text = w.nodes.iter().any(|n| {
        if let Action::Canvas { step } = &n.action {
            return crate::canvas::needs_text(step);
        }
        false
    });
    let settings = recognition::settings(&app).await?;
    let text_recognition = if needs_text && recognition::configured(&app, &settings) {
        if w.model_limit == 0 {
            w.model_limit = 10;
        }
        TextRecognition::Model
    } else {
        if needs_text && !crate::platform::has_local_ocr(&app.root, &app.data) {
            bail!("此任务需要文字判断，请配置模型密钥，或安装本地读字工具")
        }
        TextRecognition::Local
    };
    let model_settings = Some(settings);
    let d = app
        .db
        .get::<Device>("devices", device_id)
        .await?
        .context("手机未登记")?;
    if !d.online || app.adb.endpoint(device_id).await.is_err() {
        bail!("手机离线或未授权")
    }
    if !w.devices.is_empty() && !w.devices.iter().any(|id| id == device_id) {
        bail!("工作流尚未适配这台手机")
    }
    if app
        .db
        .list::<Run>("runs")
        .await?
        .iter()
        .any(|r| r.busy() && r.device_id == device_id)
    {
        bail!("这台手机已有任务")
    }
    let r = Run {
        canvas_progress: json!({}),
        id: uuid::Uuid::new_v4().to_string(),
        device_id: device_id.into(),
        node: w.entry.clone(),
        workflow: w,
        state: "running".into(),
        started_at: now(),
        ended_at: None,
        reason: "开始执行".into(),
        model_calls: 0,
        checkpoint: json!({}),
        pending: None,
        last_shot: None,
        error_detail: None,
        resume_app: None,
        resume_size: None,
        model_settings,
        text_recognition: Some(text_recognition),
    };
    app.db.put("runs", &r.id, &r).await?;
    app.event(&r, "开始执行").await?;
    if needs_text {
        app.event(
            &r,
            if text_recognition == TextRecognition::Model {
                "文字条件优先使用已配置模型；无法确认时停止，不切换本地识别"
            } else {
                "未配置模型，文字条件使用本地识别"
            },
        )
        .await?;
    }
    spawn(app.clone(), r.clone()).await;
    Ok(r)
}
async fn spawn(app: Arc<App>, r: Run) {
    let control = Arc::new(RunControl::default());
    app.controls
        .lock()
        .await
        .insert(r.id.clone(), control.clone());
    app.workers.spawn(execute(app.clone(), r, control));
}
pub async fn request_pause(app: &App, id: &str) -> Result<()> {
    let _gate = app.run_gate.lock().await;
    let r = app.db.get::<Run>("runs", id).await?.context("任务不存在")?;
    if r.state == "paused" {
        return Ok(());
    }
    let control = app
        .controls
        .lock()
        .await
        .get(id)
        .cloned()
        .context("任务未运行")?;
    control.pause.cancel();
    let _ = app.events.send(json!({"type":"changed","run_id":id}));
    Ok(())
}
pub async fn resume(app: Arc<App>, id: &str) -> Result<Run> {
    let _gate = app.run_gate.lock().await;
    let mut r = app.db.get::<Run>("runs", id).await?.context("任务不存在")?;
    if app
        .db
        .get::<Workflow>("workflows", &r.workflow.id)
        .await?
        .is_none()
    {
        bail!("任务已移除，无法继续")
    }
    if !matches!(r.state.as_str(), "paused" | "cancelled") {
        bail!("只有已暂停或已停止的任务可以继续")
    }
    if app.controls.lock().await.contains_key(id) {
        bail!("任务正在完成暂停或停止，请稍后再试")
    }
    if app
        .db
        .list::<Run>("runs")
        .await?
        .iter()
        .any(|other| other.id != r.id && other.device_id == r.device_id && other.busy())
    {
        bail!("这台手机已有其他任务，请先停止该任务再继续")
    }
    let node = r
        .workflow
        .nodes
        .iter()
        .find(|node| node.id == r.node)
        .context("当前步骤不存在")?;
    if r.pending.is_some() && matches!(node.action, Action::Canvas { .. }) {
        bail!("上次动作结果未确认，不能继续。请核对页面后重新开始，避免重复操作。")
    }
    app.adb.check_unlocked(&r.device_id).await?;
    let expected = r
        .resume_app
        .as_deref()
        .or_else(|| r.checkpoint["foreground"].as_str())
        .context("暂停或停止时未能确认原应用，请在目标页面重新开始")?;
    let current = foreground_package(&app.adb.foreground(&r.device_id).await?);
    if current.as_deref() != Some(expected) {
        bail!("请先回到暂停或停止前的应用和目标页面，再点击继续")
    }
    if let Some([width, height]) = r.resume_size {
        if app.adb.size(&r.device_id).await? != (width, height) {
            bail!("屏幕尺寸与任务启动时不同，请停止任务并在工作流编辑器重新取图标注")
        }
    }
    r.state = "recovering".into();
    r.ended_at = None;
    r.reason = "已请求继续，重新检查当前步骤".into();
    r.error_detail = None;
    save(&app, &r).await?;
    app.event(&r, "继续任务；保留进度，不补发结果未确认的动作")
        .await?;
    spawn(app.clone(), r.clone()).await;
    Ok(r)
}
pub async fn recover(app: Arc<App>) -> Result<()> {
    for mut r in app.db.list::<Run>("runs").await? {
        if r.busy() && r.state != "paused" {
            r.state = "recovering".into();
            r.reason = "后台重启，重新观察后恢复".into();
            app.db.put("runs", &r.id, &r).await?;
            spawn(app.clone(), r).await;
        }
    }
    Ok(())
}

pub(crate) async fn save(app: &App, r: &Run) -> Result<()> {
    app.db.put("runs", &r.id, r).await?;
    let _ = app.events.send(json!({"type":"changed","run_id":r.id}));
    Ok(())
}
pub(crate) async fn shot(app: &App, r: &mut Run, f: &Frame, label: &str) -> Result<()> {
    let folder = app.data.join("shots").join(&r.id);
    tokio::fs::create_dir_all(&folder).await?;
    let name = format!("{}-{label}.png", uuid::Uuid::new_v4().simple());
    tokio::fs::write(folder.join(&name), &f.png).await?;
    r.last_shot = Some(format!("{}/{}", r.id, name));
    save(app, r).await
}
pub(crate) async fn ready(app: &App, r: &mut Run) -> Result<bool> {
    let mut restored = r.state != "running";
    loop {
        check_pause(app, r).await?;
        let d = app.db.get::<Device>("devices", &r.device_id).await?;
        let connected = app.adb.endpoint(&r.device_id).await.is_ok();
        if connected && d.as_ref().is_some_and(|d| !d.locked) {
            if r.state != "running" {
                r.state = "running".into();
                r.reason = "已连接，重新检查当前画面".into();
                save(app, r).await?;
                app.event(r, &r.reason).await?;
            }
            return Ok(restored);
        }
        restored = true;
        let reason = if connected {
            "需要在手机上解锁"
        } else {
            "等待手机上线"
        };
        if r.state != "waiting_device" || r.reason != reason {
            r.state = "waiting_device".into();
            r.reason = reason.into();
            save(app, r).await?;
            app.event(r, reason).await?;
        }
        pause(app, r, 3.).await?;
    }
}
async fn check_pause(app: &App, r: &Run) -> Result<()> {
    if app
        .controls
        .lock()
        .await
        .get(&r.id)
        .is_some_and(|control| control.pause.is_cancelled())
    {
        return Err(PauseRequested.into());
    }
    Ok(())
}
pub(crate) async fn pause(app: &App, r: &Run, seconds: f64) -> Result<()> {
    let control = app
        .controls
        .lock()
        .await
        .get(&r.id)
        .cloned()
        .context("任务控制已结束")?;
    tokio::select! {
        biased;
        _ = control.pause.cancelled() => Err(PauseRequested.into()),
        _ = tokio::time::sleep(Duration::from_secs_f64(seconds.max(0.))) => Ok(()),
    }
}

async fn execute(app: Arc<App>, mut r: Run, control: Arc<RunControl>) {
    let outcome = tokio::select! {
        biased;
        _=control.stop.cancelled()=>Ok("cancelled"),
        _=app.stop.cancelled()=>if control.pause.is_cancelled() {Err(PauseRequested.into())} else {Ok("recovering")},
        result=run_steps(&app,&mut r)=>result,
    };
    match outcome {
        Ok(state) => {
            r.state = state.into();
            r.reason = match state {
                "cancelled" => "用户停止",
                "recovering" => "后台停止，等待恢复",
                "needs_help" => "流程标记需要处理",
                _ => "任务完成",
            }
            .into();
        }
        Err(e) => {
            if e.is::<PauseRequested>() || e.is::<crate::adb::AdbIssue>() {
                r.state = "paused".into();
                r.reason = e
                    .downcast_ref::<crate::adb::AdbIssue>()
                    .map(|issue| issue.summary().to_owned())
                    .unwrap_or_else(|| e.to_string());
                r.error_detail = e
                    .downcast_ref::<crate::adb::AdbIssue>()
                    .map(|_| e.to_string());
            } else {
                r.state = "needs_help".into();
                r.reason = e.to_string();
            }
            if r.state != "paused" {
                if let Ok(f) = recognition::capture(&app, &r.device_id).await {
                    let _ = shot(&app, &mut r, &f, "error").await;
                }
            }
        }
    }
    if matches!(r.state.as_str(), "paused" | "cancelled") {
        r.resume_app = r.checkpoint["foreground"].as_str().map(str::to_owned);
        if r.resume_app.is_none() {
            r.resume_app =
                tokio::time::timeout(Duration::from_secs(3), app.adb.foreground(&r.device_id))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .and_then(|activity| foreground_package(&activity));
        }
    }
    if !r.busy() {
        r.ended_at = Some(now());
    }
    let _gate = app.run_gate.lock().await;
    let _ = save(&app, &r).await;
    let _ = app.event(&r, &r.reason).await;
    app.controls.lock().await.remove(&r.id);
}

async fn run_steps(app: &App, r: &mut Run) -> Result<&'static str> {
    let mut steps = 0u32;
    loop {
        ready(app, r).await?;
        steps += 1;
        if steps > 10000 {
            bail!("步骤执行超过上限")
        }
        let n = r
            .workflow
            .nodes
            .iter()
            .find(|n| n.id == r.node)
            .cloned()
            .context("当前步骤不存在")?;
        app.event(r, &n.label).await?;
        let result = step(app, r, &n).await;
        match result {
            Ok(Some(state)) => return Ok(state),
            Ok(None) => {}
            Err(e) => {
                check_pause(app, r).await?;
                if e.is::<crate::adb::AdbIssue>() {
                    return Err(e);
                }
                if app.adb.endpoint(&r.device_id).await.is_err() {
                    ready(app, r).await?;
                    continue;
                }
                return Err(e);
            }
        }
        r.pending = None;
        r.checkpoint = json!({});
        save(app, r).await?;
    }
}
async fn step(app: &App, r: &mut Run, n: &Node) -> Result<Option<&'static str>> {
    match &n.action {
        Action::Canvas { step } => crate::canvas::execute(app, r, step).await?,
        Action::End { result } => {
            if let Ok(frame) = recognition::capture(app, &r.device_id).await {
                shot(app, r, &frame, "finish").await?;
            }
            return Ok(Some(if result == "succeeded" {
                "succeeded"
            } else {
                "needs_help"
            }));
        }
    }
    Ok(None)
}

fn foreground_package(activity: &str) -> Option<String> {
    activity.split_whitespace().find_map(|value| {
        value
            .split_once('/')
            .map(|(package, _)| package.trim_start_matches('{').to_owned())
    })
}
