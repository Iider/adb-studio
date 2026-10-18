use crate::{
    App,
    recognition::{self, Frame},
    types::*,
};
use anyhow::{Context, Result, bail};
use image::GenericImageView;
use serde_json::json;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub fn builtins() -> Vec<Workflow> {
    [
        include_str!("../workflows/swipe.json"),
        include_str!("../workflows/point-click.json"),
        include_str!("../workflows/feed-duck-fixed.json"),
        include_str!("../workflows/qishui.json"),
    ]
    .iter()
    .map(|source| serde_json::from_str(source).expect("内置工作流格式错误"))
    .collect()
}
pub fn validate(w: &Workflow) -> Result<()> {
    if w.schema_version != 1
        || w.version == 0
        || w.id.is_empty()
        || w.id.len() > 80
        || !w
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
    {
        bail!("工作流编号或版本无效")
    }
    if w.name.trim().is_empty()
        || w.name.len() > 240
        || w.nodes.is_empty()
        || w.nodes.len() > 200
        || w.model_limit > 100
    {
        bail!("工作流名称、步骤数量或模型上限无效")
    }
    let ids: HashSet<_> = w.nodes.iter().map(|n| n.id.as_str()).collect();
    if ids.len() != w.nodes.len()
        || !ids.contains(w.entry.as_str())
        || ids.iter().any(|s| s.is_empty())
    {
        bail!("步骤编号重复、为空或入口不存在")
    }
    let coord = |p: &[f64]| p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v));
    let region = |r: &[f64; 4]| coord(r) && r[0] < r[2] && r[1] < r[3];
    let mut refs = vec![];
    for n in &w.nodes {
        if n.label.trim().is_empty() {
            bail!("步骤需要中文说明")
        }
        if let Some(next) = &n.next {
            refs.push(next.as_str());
        }
        match &n.action {
            Action::FixedTapLoop {
                x,
                y,
                count,
                interval,
            } => {
                if *x > 10000
                    || *y > 10000
                    || *count == 0
                    || *count > 10000
                    || !interval.is_finite()
                    || *interval < 1.
                    || *interval > 86400.
                {
                    bail!("固定点击需要有效坐标、1 到 10000 次和 1 到 86400 秒间隔")
                }
            }
            Action::TapLoop {
                text,
                guard,
                package,
                region: area,
                count,
                interval,
            } => {
                if text.is_empty()
                    || guard.is_empty()
                    || !package.contains('.')
                    || !region(area)
                    || *count == 0
                    || *count > 10000
                    || !interval.is_finite()
                    || *interval < 1.
                    || *interval > 86400.
                {
                    bail!("点击任务需要页面依据、按钮区域、1 到 10000 次和 1 到 86400 秒间隔")
                }
            }
            Action::Wait { seconds } => {
                if !seconds.is_finite() || *seconds < 0. || *seconds > 86400. {
                    bail!("等待时间无效")
                }
            }
            Action::Launch { package } => {
                if !package.contains('.')
                    || !package
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._".contains(c))
                {
                    bail!("应用编号无效")
                }
            }
            Action::TapText { text, verify } => {
                if text.is_empty() || verify.is_empty() {
                    bail!("点击需要定位文字和结果验证文字")
                }
            }
            Action::TapPoint {
                x,
                y,
                guard,
                verify,
            } => {
                if !coord(&[*x, *y]) || guard.is_empty() || verify.is_empty() {
                    bail!("点按需要有效位置、页面依据和结果验证")
                }
            }
            Action::Key { key, verify } => {
                if !["BACK", "HOME", "ENTER"].contains(&key.as_str()) || verify.is_empty() {
                    bail!("按键或结果验证无效")
                }
            }
            Action::Swipe {
                from,
                to,
                duration_ms,
            } => {
                if !coord(from) || !coord(to) || !(100..=2000).contains(duration_ms) {
                    bail!("滑动位置或时间无效")
                }
            }
            Action::SwipeLoop {
                interval,
                count,
                minutes,
                ..
            } => {
                if !interval.is_finite()
                    || *interval <= 0.
                    || *interval > 86400.
                    || count.is_some_and(|v| v == 0)
                    || minutes.is_some_and(|v| !v.is_finite() || v <= 0.)
                    || (count.is_none() && minutes.is_none())
                {
                    bail!("滑动任务需有有效间隔和次数或时长")
                }
            }
            Action::Branch {
                cases, otherwise, ..
            } => {
                if cases.is_empty() || cases.iter().any(|c| c.contains.is_empty()) {
                    bail!("判断分支为空")
                };
                refs.push(otherwise);
                refs.extend(cases.iter().map(|c| c.next.as_str()));
            }
            Action::QishuiLoop {
                coin_region,
                identity_region,
                warmup,
                stall,
                finish_stall,
                finish_swipes,
            } => {
                if !region(coin_region)
                    || !region(identity_region)
                    || ![warmup, stall, finish_stall]
                        .iter()
                        .all(|v| v.is_finite() && **v >= 1. && **v <= 120.)
                    || *finish_stall < *stall
                    || *finish_swipes < 3
                    || *finish_swipes > 10
                {
                    bail!("汽水任务识别区域或完成条件无效")
                }
            }
            Action::End { result } => {
                if !["succeeded", "needs_help"].contains(&result.as_str()) {
                    bail!("结束状态无效")
                }
            }
            _ => {}
        }
        if !matches!(n.action, Action::End { .. } | Action::Branch { .. }) && n.next.is_none() {
            bail!("步骤缺少下一步")
        }
    }
    if refs.iter().any(|s| !ids.contains(s)) {
        bail!("下一步不存在")
    }
    let mut reached = HashSet::new();
    let mut todo = vec![w.entry.as_str()];
    while let Some(id) = todo.pop() {
        if !reached.insert(id) {
            continue;
        };
        let n = w.nodes.iter().find(|n| n.id == id).unwrap();
        if let Some(next) = &n.next {
            todo.push(next)
        };
        if let Action::Branch {
            cases, otherwise, ..
        } = &n.action
        {
            todo.push(otherwise);
            todo.extend(cases.iter().map(|c| c.next.as_str()));
        }
    }
    if !w
        .nodes
        .iter()
        .any(|n| reached.contains(n.id.as_str()) && matches!(n.action, Action::End { .. }))
    {
        bail!("入口无法到达结束步骤")
    }
    Ok(())
}
fn branch_choice<'a>(
    cases: &'a [Branch],
    otherwise: &'a str,
    guard: Option<&str>,
    text: &str,
) -> Option<&'a str> {
    if guard.is_some_and(|g| !text.contains(g)) {
        return None;
    }
    cases
        .iter()
        .find(|c| text.contains(&c.contains))
        .map(|c| c.next.as_str())
        .or_else(|| guard.map(|_| otherwise))
}
pub fn samples(w: &Workflow) -> Result<()> {
    validate(w)?;
    if w.tests.is_empty() {
        bail!("缺少样本验证")
    }
    for t in &w.tests {
        let n = w
            .nodes
            .iter()
            .find(|n| n.id == t.node)
            .context("样本步骤不存在")?;
        let actual = match &n.action {
            Action::Branch {
                cases,
                otherwise,
                guard,
            } => branch_choice(cases, otherwise, guard.as_deref(), &t.text).unwrap_or("unknown"),
            Action::QishuiLoop { .. } => {
                if recognition::blocked(&t.text) {
                    "blocked"
                } else if recognition::video(&t.text) {
                    "video"
                } else {
                    "unknown"
                }
            }
            Action::QishuiNavigate => {
                if t.text.contains("连续刷视频赚金币") {
                    "task"
                } else {
                    "unknown"
                }
            }
            Action::TapLoop { text, guard, .. } => {
                if t.text.contains(guard) && t.text.split_whitespace().any(|word| word == text) {
                    "ready"
                } else {
                    "unknown"
                }
            }
            Action::SwipeLoop { .. } | Action::FixedTapLoop { .. } => "valid",
            _ => bail!("此步骤的样本验证类型未提供"),
        };
        if actual != t.expected {
            bail!(
                "样本未通过：{}，期望 {}，实际 {}",
                t.node,
                t.expected,
                actual
            )
        }
    }
    for n in &w.nodes {
        if matches!(
            n.action,
            Action::Branch { .. } | Action::QishuiLoop { .. } | Action::QishuiNavigate
        ) && !w.tests.iter().any(|t| t.node == n.id)
        {
            bail!("判断步骤缺少样本：{}", n.id)
        }
    }
    Ok(())
}
pub async fn active(app: &App, id: &str) -> Result<Workflow> {
    let key = app
        .db
        .get::<String>("active", id)
        .await?
        .context("工作流尚未启用，请先试跑")?;
    app.db
        .get("workflows", &key)
        .await?
        .context("工作流版本不存在")
}
pub async fn activate(app: &App, w: &Workflow, automatic: bool) -> Result<bool> {
    let successful = app
        .db
        .list::<Run>("runs")
        .await?
        .into_iter()
        .filter(|r| r.trial && r.state == "succeeded" && r.workflow.key() == w.key())
        .collect::<Vec<_>>();
    let tested = if w.devices.is_empty() {
        !successful.is_empty()
    } else {
        w.devices
            .iter()
            .all(|id| successful.iter().any(|r| &r.device_id == id))
    };
    if !w.sample_validated || !tested {
        bail!("需要样本验证及适用手机的成功试跑记录")
    }
    if automatic {
        if let Ok(current) = active(app, &w.id).await {
            if current.version >= w.version {
                return Ok(false);
            }
        }
    }
    app.db.put("active", &w.id, &w.key()).await?;
    let _ = app.events.send(json!({"type":"changed"}));
    Ok(true)
}
pub fn with_tap_settings(mut w: Workflow, settings: Option<&TapSettings>) -> Result<Workflow> {
    if let Some(settings) = settings {
        let mut found = false;
        for node in &mut w.nodes {
            if let Action::TapLoop {
                count, interval, ..
            }
            | Action::FixedTapLoop {
                count, interval, ..
            } = &mut node.action
            {
                *count = settings.count;
                *interval = settings.interval;
                node.label = format!("点击 {} 次，间隔 {} 秒", count, interval);
                found = true;
            }
        }
        if !found {
            bail!("此工作流不支持点击设置")
        }
    }
    validate(&w)?;
    Ok(w)
}

pub fn with_swipe_settings(mut w: Workflow, settings: Option<&SwipeSettings>) -> Result<Workflow> {
    if let Some(settings) = settings {
        if !settings.minutes.is_finite()
            || settings.minutes <= 0.0
            || settings.minutes > 10080.0
            || !settings.interval.is_finite()
            || settings.interval <= 0.0
            || settings.interval > 86400.0
        {
            bail!("总时长需为 0 到 10080 分钟，滑动间隔需为 0 到 86400 秒（均大于 0）")
        }
        let mut found = false;
        for node in &mut w.nodes {
            if let Action::SwipeLoop {
                interval,
                count,
                minutes,
                ..
            } = &mut node.action
            {
                *interval = settings.interval;
                *count = None;
                *minutes = Some(settings.minutes);
                node.label = format!(
                    "每隔 {} 秒上滑，运行 {} 分钟",
                    settings.interval, settings.minutes
                );
                found = true;
            }
        }
        if !found {
            bail!("此工作流不支持滑动设置")
        }
        w.description = format!(
            "保持当前页面，先等待，每隔 {} 秒上滑，运行 {} 分钟。",
            settings.interval, settings.minutes
        );
    }
    validate(&w)?;
    Ok(w)
}

pub async fn start(app: Arc<App>, device_id: &str, mut w: Workflow, trial: bool) -> Result<Run> {
    let _gate = app.run_gate.lock().await;
    validate(&w)?;
    let d = app
        .db
        .get::<Device>("devices", device_id)
        .await?
        .context("手机未登记")?;
    if w.id == "point-click" {
        let point = d.tap_point.as_ref().context("请先为这台手机标注点击位置")?;
        point.validate()?;
        for node in &mut w.nodes {
            if let Action::FixedTapLoop { x, y, .. } = &mut node.action {
                *x = point.x;
                *y = point.y;
                node.label = format!("{}；位置 ({}, {})", node.label, x, y);
            }
        }
    }
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
        id: uuid::Uuid::new_v4().to_string(),
        device_id: device_id.into(),
        node: w.entry.clone(),
        workflow: w,
        state: "running".into(),
        started_at: now(),
        ended_at: None,
        reason: "开始执行".into(),
        model_calls: 0,
        trial,
        checkpoint: json!({}),
        pending: None,
        last_shot: None,
    };
    app.db.put("runs", &r.id, &r).await?;
    app.event(&r, "开始执行").await?;
    spawn(app.clone(), r.clone()).await;
    Ok(r)
}
async fn spawn(app: Arc<App>, r: Run) {
    let stop = CancellationToken::new();
    app.controls.lock().await.insert(r.id.clone(), stop.clone());
    app.workers.spawn(execute(app.clone(), r, stop));
}
pub async fn recover(app: Arc<App>) -> Result<()> {
    for mut r in app.db.list::<Run>("runs").await? {
        if r.busy() {
            r.state = "recovering".into();
            r.reason = "后台重启，重新观察后恢复".into();
            app.db.put("runs", &r.id, &r).await?;
            spawn(app.clone(), r).await;
        }
    }
    Ok(())
}

async fn save(app: &App, r: &Run) -> Result<()> {
    app.db.put("runs", &r.id, r).await?;
    let _ = app.events.send(json!({"type":"changed","run_id":r.id}));
    Ok(())
}
async fn shot(app: &App, r: &mut Run, f: &Frame, label: &str) -> Result<()> {
    let folder = app.data.join("shots").join(&r.id);
    tokio::fs::create_dir_all(&folder).await?;
    let name = format!("{}-{label}.png", uuid::Uuid::new_v4().simple());
    tokio::fs::write(folder.join(&name), &f.png).await?;
    r.last_shot = Some(format!("{}/{}", r.id, name));
    save(app, r).await
}
async fn ready(app: &App, r: &mut Run) -> Result<bool> {
    let mut restored = r.state != "running";
    loop {
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
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
async fn pause(seconds: f64) {
    tokio::time::sleep(Duration::from_secs_f64(seconds.max(0.))).await;
}

async fn execute(app: Arc<App>, mut r: Run, stop: CancellationToken) {
    let outcome = tokio::select! {
        _=stop.cancelled()=>Ok("cancelled"),
        _=app.stop.cancelled()=>Ok("recovering"),
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
            r.state = "needs_help".into();
            r.reason = e.to_string();
            if !fixed_click_workflow(&r.workflow) {
                if let Ok(f) = recognition::frame(&app, &r.device_id).await {
                    let _ = shot(&app, &mut r, &f, "error").await;
                }
            }
        }
    }
    if !r.busy() {
        r.ended_at = Some(now());
    }
    let _ = save(&app, &r).await;
    let _ = app.event(&r, &r.reason).await;
    if r.trial && r.state == "succeeded" {
        let _gate = app.run_gate.lock().await;
        if let Ok(Some(wf)) = app.db.get::<Workflow>("workflows", &r.workflow.key()).await {
            if activate(&app, &wf, true).await.unwrap_or(false) {
                let _ = app.event(&r, "样本验证与真机试跑通过，已自动启用").await;
            }
        }
    }
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
                if app.adb.endpoint(&r.device_id).await.is_err() {
                    ready(app, r).await?;
                    continue;
                }
                return Err(e);
            }
        }
        if !matches!(n.action, Action::Branch { .. }) {
            r.node = n.next.context("缺少下一步")?;
        }
        r.pending = None;
        r.checkpoint = json!({});
        save(app, r).await?;
    }
}
async fn step(app: &App, r: &mut Run, n: &Node) -> Result<Option<&'static str>> {
    match &n.action {
        Action::End { result } => {
            if !fixed_click_workflow(&r.workflow) {
                if let Ok(f) = recognition::frame(app, &r.device_id).await {
                    shot(app, r, &f, "finish").await?;
                }
            }
            return Ok(Some(if result == "succeeded" {
                "succeeded"
            } else {
                "needs_help"
            }));
        }
        Action::FixedTapLoop {
            x,
            y,
            count,
            interval,
        } => fixed_tap_loop(app, r, *x, *y, *count, *interval).await?,
        Action::TapLoop {
            text,
            guard,
            package,
            region,
            count,
            interval,
        } => {
            tap_loop(app, r, text, guard, package, *region, *count, *interval).await?;
        }
        Action::Wait { seconds } => pause(*seconds).await,
        Action::Launch { package } => {
            if !app.adb.foreground(&r.device_id).await?.contains(package) {
                r.pending = Some("launch".into());
                save(app, r).await?;
                app.adb
                    .phone(
                        &r.device_id,
                        &[
                            "shell",
                            "monkey",
                            "-p",
                            package,
                            "-c",
                            "android.intent.category.LAUNCHER",
                            "1",
                        ],
                    )
                    .await?;
                pause(2.).await;
            }
            if !app.adb.foreground(&r.device_id).await?.contains(package) {
                bail!("未确认应用已打开")
            }
        }
        Action::TapText { text, verify } => {
            let f = recognition::frame(app, &r.device_id).await?;
            if r.pending.is_some() {
                if f.text().contains(verify) {
                    return Ok(None);
                }
                bail!("上次点击结果未确认，需要处理")
            }
            let words = recognition::ui_words(app, &r.device_id)
                .await
                .unwrap_or_default();
            let matches: Vec<_> = words.iter().filter(|w| w.text == *text).collect();
            let matches = if matches.is_empty() {
                f.words
                    .iter()
                    .filter(|w| w.text == *text && w.confidence >= 0.5)
                    .collect()
            } else {
                matches
            };
            if matches.len() != 1 {
                bail!("定位文字不唯一：{text}")
            }
            let m = matches[0];
            r.pending = Some("tap".into());
            save(app, r).await?;
            app.adb.tap(&r.device_id, m.x, m.y).await?;
            pause(2.).await;
            let after = recognition::frame(app, &r.device_id).await?;
            shot(app, r, &after, "tap").await?;
            if !after.text().contains(verify) {
                bail!("点击后的画面不符")
            }
        }
        Action::TapPoint {
            x,
            y,
            guard,
            verify,
        } => {
            let f = recognition::frame(app, &r.device_id).await?;
            if r.pending.is_some() {
                if f.text().contains(verify) {
                    return Ok(None);
                }
                bail!("上次点击结果未确认，需要处理")
            }
            if !f.text().contains(guard) {
                bail!("点击前的画面不符")
            }
            let (w, h) = f.image.dimensions();
            r.pending = Some("tap".into());
            save(app, r).await?;
            app.adb
                .tap(&r.device_id, (*x * w as f64) as u32, (*y * h as f64) as u32)
                .await?;
            pause(2.).await;
            let after = recognition::frame(app, &r.device_id).await?;
            shot(app, r, &after, "tap").await?;
            if !after.text().contains(verify) {
                bail!("点击后的画面不符")
            }
        }
        Action::Key { key, verify } => {
            let f = recognition::frame(app, &r.device_id).await?;
            if r.pending.is_some() {
                if f.text().contains(verify) {
                    return Ok(None);
                }
                bail!("上次按键结果未确认，需要处理")
            }
            r.pending = Some("key".into());
            save(app, r).await?;
            app.adb
                .phone(
                    &r.device_id,
                    &["shell", "input", "keyevent", &format!("KEYCODE_{key}")],
                )
                .await?;
            pause(1.).await;
            if !recognition::frame(app, &r.device_id)
                .await?
                .text()
                .contains(verify)
            {
                bail!("按键后的画面不符")
            }
        }
        Action::Swipe {
            from,
            to,
            duration_ms,
        } => {
            if r.pending.is_some() {
                bail!("上次滑动结果未确认，需要处理")
            }
            r.pending = Some("swipe".into());
            save(app, r).await?;
            app.adb
                .swipe(&r.device_id, *from, *to, *duration_ms)
                .await?;
        }
        Action::Branch {
            cases,
            otherwise,
            guard,
        } => {
            let f = recognition::frame(app, &r.device_id).await?;
            if let Some(next) = branch_choice(cases, otherwise, guard.as_deref(), &f.text()) {
                let label = r
                    .workflow
                    .nodes
                    .iter()
                    .find(|node| node.id == next)
                    .map(|node| node.label.clone())
                    .unwrap_or_else(|| next.into());
                app.event(r, &format!("本机规则 → {label}")).await?;
                r.node = next.into();
            } else {
                let mut choices = cases.iter().map(|c| c.next.as_str()).collect::<Vec<_>>();
                choices.push(otherwise);
                choices.push("unknown");
                let v = recognition::model(
                    app,
                    r,
                    &[&f.png],
                    &format!(
                        "页面依据：{:?}。判断符合哪个分支：{}。页面依据不符合时选 unknown",
                        guard,
                        serde_json::to_string(cases)?
                    ),
                    &choices,
                )
                .await?;
                r.node = v.choice;
            }
        }
        Action::SwipeLoop {
            interval,
            count,
            minutes,
            long,
        } => swipe_loop(app, r, *interval, *count, *minutes, *long).await?,
        Action::QishuiNavigate => navigate(app, r).await?,
        Action::QishuiLoop {
            coin_region,
            identity_region,
            warmup,
            stall,
            finish_stall,
            finish_swipes,
        } => {
            earn(
                app,
                r,
                *coin_region,
                *identity_region,
                *warmup,
                *stall,
                *finish_stall,
                *finish_swipes,
            )
            .await?
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
fn fixed_click_workflow(w: &Workflow) -> bool {
    w.nodes
        .iter()
        .any(|node| matches!(node.action, Action::FixedTapLoop { .. }))
}

async fn fixed_tap_loop(
    app: &App,
    r: &mut Run,
    x: u32,
    y: u32,
    count: u32,
    interval: f64,
) -> Result<()> {
    if r.pending.is_some() {
        bail!("上次点击是否成功未确认，已停止以避免重复点击")
    }
    let mut done = r.checkpoint["done"].as_u64().unwrap_or(0);
    let mut last_click: Option<tokio::time::Instant> = None;
    while done < u64::from(count) {
        if let Some(last) = last_click {
            tokio::time::sleep_until(last + Duration::from_secs_f64(interval)).await;
        }
        ready(app, r).await?;
        r.pending = Some("fixed_tap".into());
        save(app, r).await?;
        app.adb.tap(&r.device_id, x, y).await?;
        last_click = Some(tokio::time::Instant::now());
        done += 1;
        r.pending = None;
        r.checkpoint = json!({"done": done});
        save(app, r).await?;
        app.event(r, &format!("已点击 {done}/{count} 次；固定位置 ({x}, {y})"))
            .await?;
    }
    Ok(())
}

fn tap_target(frame: &Frame, text: &str, guard: &str, area: [f64; 4]) -> Result<(u32, u32)> {
    if !frame.text().contains(guard) {
        bail!("已离开目标页面，请重新打开招财鸭")
    }
    let (width, height) = frame.image.dimensions();
    let matches: Vec<_> = frame
        .words
        .iter()
        .filter(|word| {
            word.text == text
                && word.confidence >= 0.5
                && (word.x as f64) >= area[0] * width as f64
                && (word.x as f64) <= area[2] * width as f64
                && (word.y as f64) >= area[1] * height as f64
                && (word.y as f64) <= area[3] * height as f64
        })
        .collect();
    if matches.len() != 1 {
        bail!("未找到唯一的{}按钮，可能出现弹窗或饲料不足", text)
    }
    Ok((matches[0].x, matches[0].y))
}

#[allow(clippy::too_many_arguments)]
async fn tap_loop(
    app: &App,
    r: &mut Run,
    text: &str,
    guard: &str,
    package: &str,
    area: [f64; 4],
    count: u32,
    interval: f64,
) -> Result<()> {
    if r.pending.is_some() {
        bail!("上次点击结果未确认，已停止以避免重复点击")
    }
    let mut done = r.checkpoint["done"].as_u64().unwrap_or(0);
    while done < u64::from(count) {
        if done > 0 {
            pause(interval).await;
        }
        ready(app, r).await?;
        if !app.adb.foreground(&r.device_id).await?.contains(package) {
            bail!("当前应用已改变")
        }
        let before = recognition::frame(app, &r.device_id).await?;
        let (x, y) = match tap_target(&before, text, guard, area) {
            Ok(point) => point,
            Err(error) => {
                shot(app, r, &before, "button-missing").await?;
                return Err(error);
            }
        };
        if done == 0 {
            shot(app, r, &before, "start").await?;
        }
        r.pending = Some("tap_loop".into());
        save(app, r).await?;
        app.adb.tap(&r.device_id, x, y).await?;
        pause(0.5).await;
        let after = recognition::frame(app, &r.device_id).await?;
        if let Err(error) = tap_target(&after, text, guard, area) {
            shot(app, r, &after, "verify-failed").await?;
            return Err(error);
        }
        done += 1;
        r.pending = None;
        r.checkpoint = json!({"done": done});
        save(app, r).await?;
        app.event(r, &format!("已点击 {done}/{count} 次；依据：{guard}页面、{text}按钮 ({x}, {y})；点击后页面复核通过")).await?;
    }
    Ok(())
}

async fn swipe_loop(
    app: &App,
    r: &mut Run,
    interval: f64,
    count: Option<u32>,
    minutes: Option<f64>,
    long: bool,
) -> Result<()> {
    let current =
        foreground_package(&app.adb.foreground(&r.device_id).await?).context("未读到当前应用")?;
    if let Some(expected) = r.checkpoint["foreground"].as_str() {
        if current != expected {
            bail!("当前应用已改变，请在目标页面重新开始")
        }
    }
    r.checkpoint["foreground"] = json!(current);
    save(app, r).await?;
    let mut done = r.checkpoint["done"].as_u64().unwrap_or(0);
    let mut elapsed = r.checkpoint["elapsed"].as_f64().unwrap_or(0.);
    // 未确认的滑动只计入一次，恢复后重新等待，不重放。
    if r.pending.is_some() {
        r.pending = None;
        save(app, r).await?;
    }
    loop {
        if count.is_some_and(|n| done >= u64::from(n))
            || minutes.is_some_and(|m| elapsed >= m * 60.)
        {
            return Ok(());
        }
        let wait = minutes
            .map(|m| interval.min((m * 60. - elapsed).max(0.)))
            .unwrap_or(interval);
        pause(wait).await;
        elapsed += wait;
        ready(app, r).await?;
        if foreground_package(&app.adb.foreground(&r.device_id).await?).as_deref()
            != Some(current.as_str())
        {
            bail!("当前应用已改变，请在目标页面重新开始")
        };
        if wait < interval {
            r.checkpoint = json!({"done":done,"elapsed":elapsed,"foreground":current});
            save(app, r).await?;
            return Ok(());
        }
        done += 1;
        r.pending = Some("swipe".into());
        r.checkpoint = json!({"done":done,"elapsed":elapsed,"foreground":current});
        save(app, r).await?;
        app.adb
            .swipe(
                &r.device_id,
                [0.5, if long { 0.88 } else { 0.78 }],
                [0.5, if long { 0.15 } else { 0.32 }],
                250,
            )
            .await?;
        r.pending = None;
        save(app, r).await?;
        app.event(r, &format!("已滑动 {done} 次")).await?;
    }
}

async fn qishui_version(app: &App, id: &str) -> Result<()> {
    let package = app
        .adb
        .phone_text(id, &["shell", "dumpsys", "package", "com.luna.music"])
        .await?;
    if !package.lines().any(|l| l.trim() == "versionName=21.1.0") {
        bail!("汽水音乐版本改变或未安装，请 Agent 重新适配")
    }
    Ok(())
}

async fn navigate(app: &App, r: &mut Run) -> Result<()> {
    qishui_version(app, &r.device_id).await?;
    for _ in 0..14 {
        ready(app, r).await?;
        if !app
            .adb
            .foreground(&r.device_id)
            .await?
            .contains("com.luna.music")
        {
            bail!("当前应用不是汽水音乐")
        }
        let f = recognition::frame(app, &r.device_id).await?;
        let text = f.text();
        shot(app, r, &f, "navigate").await?;
        if recognition::video(&text) {
            r.pending = None;
            return Ok(());
        }
        if recognition::blocked(&text) {
            app.event(r, "页面加载或网络异常，等待复查").await?;
            pause(5.).await;
            continue;
        }
        let (w, h) = f.image.dimensions();
        let mut target = None;
        if text.contains("签到7天领") && text.contains("打开签到提醒") {
            target = Some(((w as f64 * 0.5) as u32, (h as f64 * 0.7375) as u32));
        } else if let Some(label) = f.words.iter().find(|v| v.text.contains("连续刷视频赚金币"))
        {
            target = Some((label.x, label.y));
        } else if text.contains("模式选择") && !text.contains("做任务") {
            target = Some(((w as f64 * 0.788) as u32, (h as f64 * 0.075) as u32));
        }
        if let Some((x, y)) = target {
            let fresh = recognition::frame(app, &r.device_id).await?;
            if recognition::difference(
                &f.signature([0., 0.04, 1., 0.10], false),
                &fresh.signature([0., 0.04, 1., 0.10], false),
            ) > 8.
            {
                continue;
            }
            r.pending = Some("navigate".into());
            save(app, r).await?;
            app.adb.tap(&r.device_id, x, y).await?;
            pause(2.).await;
            continue;
        }
        let v=recognition::model(app,r,&[&f.png],"寻找汽水音乐的金币入口、连续刷视频赚金币文字入口，或者可关闭的遮挡弹窗。宝箱、提现、广告膨胀、下载按钮不是目标。选择 coin_entry/task_entry/close_popup 并返回按钮中心；视频任务选择 video",&["coin_entry","task_entry","close_popup","video","unknown"]).await?;
        if v.choice == "video" {
            return Ok(());
        }
        let [x, y] = v.target.context("模型未返回定位")?;
        if x >= w || y >= h {
            bail!("模型位置无效")
        }
        let fresh = recognition::frame(app, &r.device_id).await?;
        if recognition::difference(
            &f.signature([0., 0., 1., 1.], false),
            &fresh.signature([0., 0., 1., 1.], false),
        ) > 8.
        {
            continue;
        }
        r.pending = Some("navigate".into());
        save(app, r).await?;
        app.adb.tap(&r.device_id, x, y).await?;
        pause(2.).await;
    }
    bail!("未能进入视频任务，需要处理")
}

#[derive(Default, Debug)]
pub struct Earnings {
    pub last_coin: Option<u64>,
    pub idle: f64,
    pub since_swipe: f64,
    pub failures: u32,
    pub identity: String,
}
impl Earnings {
    pub fn observe(&mut self, coin: Option<u64>, motion: bool, seconds: f64) {
        if coin.zip(self.last_coin).is_some_and(|(a, b)| a > b) || motion {
            self.idle = 0.;
            self.failures = 0;
        } else {
            self.idle += seconds;
        }
        if coin.is_some() {
            self.last_coin = coin;
        }
        self.since_swipe += seconds;
    }
    pub fn finished(&self, stall: f64, swipes: u32) -> bool {
        self.idle >= stall && self.failures >= swipes
    }
}
async fn earn(
    app: &App,
    r: &mut Run,
    coin_region: [f64; 4],
    identity_region: [f64; 4],
    warmup: f64,
    stall: f64,
    finish_stall: f64,
    finish_swipes: u32,
) -> Result<()> {
    qishui_version(app, &r.device_id).await?;
    let mut state = Earnings::default();
    let mut previous: Option<Vec<u8>> = None;
    let mut evidence: Vec<Vec<u8>> = vec![];
    let mut entered = false;
    r.pending = None;
    save(app, r).await?;
    pause(warmup).await;
    let mut observed_at: Option<tokio::time::Instant> = None;
    let mut sampler = tokio::time::interval(Duration::from_secs(1));
    sampler.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        sampler.tick().await;
        let restored = ready(app, r).await?;
        if restored {
            state = Earnings::default();
            previous = None;
            evidence.clear();
            observed_at = None;
            pause(warmup).await;
        }
        if !app
            .adb
            .foreground(&r.device_id)
            .await?
            .contains("com.luna.music")
        {
            bail!("已离开汽水音乐，需要处理")
        }
        let mut f = recognition::frame(app, &r.device_id).await?;
        let text = f.text();
        if recognition::blocked(&text) {
            app.event(r, "网络或加载异常，暂停完成判断").await?;
            state = Earnings::default();
            previous = None;
            pause(5.).await;
            continue;
        }
        if !recognition::video(&text) {
            let v = recognition::model(
                app,
                r,
                &[&f.png],
                "确认是否为汽水音乐金币视频任务，或网络/加载异常。画面需有金币计数和赚钱状态",
                &["video", "loading", "error", "unknown"],
            )
            .await?;
            if v.choice == "error" {
                bail!("当前页面不是可继续的视频任务，需要处理")
            }
            if v.choice != "video" {
                state = Earnings::default();
                previous = None;
                pause(5.).await;
                continue;
            }
        }
        if !entered {
            shot(app, r, &f, "video").await?;
            entered = true;
        }
        let mut coin = f.coin(coin_region);
        if coin.is_none() {
            if f.region_text(coin_region).contains("赚钱中") {
                state.idle = 0.;
                state.failures = 0;
                previous = None;
                pause(1.).await;
                continue;
            }
            let v = recognition::model(
                app,
                r,
                &[&f.png],
                "读取右上角金币计数，写入 coin 字段；数字明确选 coin，加载或网络异常选 loading",
                &["coin", "loading", "unknown"],
            )
            .await?;
            if v.choice != "coin" {
                state = Earnings::default();
                previous = None;
                pause(5.).await;
                continue;
            }
            let fresh = recognition::frame(app, &r.device_id).await?;
            if recognition::blocked(&fresh.text())
                || !recognition::video(&fresh.text())
                || recognition::difference(
                    &f.signature(coin_region, true),
                    &fresh.signature(coin_region, true),
                ) > 1.2
            {
                state = Earnings::default();
                previous = None;
                pause(1.).await;
                continue;
            }
            coin = Some(v.coin.context("未识别到金币数字")?);
            f = fresh;
        }
        let signature = f.signature(coin_region, true);
        let coin_text = f.region_text(coin_region);
        let motion = if coin_text.contains("赚钱中") {
            true
        } else if ["已暂停", "赚钱暂停", "暂停赚"]
            .iter()
            .any(|label| coin_text.contains(label))
        {
            false
        } else {
            previous
                .as_ref()
                .is_some_and(|p| recognition::difference(p, &signature) > 1.2)
        };
        previous = Some(signature);
        let timestamp = tokio::time::Instant::now();
        let seconds = observed_at
            .map(|previous| timestamp.duration_since(previous).as_secs_f64())
            .unwrap_or(0.);
        observed_at = Some(timestamp);
        state.observe(coin, motion, seconds);
        let identity = f.region_text(identity_region);
        if state.identity.is_empty() {
            state.identity = identity.clone();
        }
        r.checkpoint["coin"] = json!(state.last_coin);
        r.checkpoint["idle"] = json!(state.idle);
        r.checkpoint["failed_swipes"] = json!(state.failures);
        r.checkpoint["video_identity"] = json!(identity);
        save(app, r).await?;
        if state.finished(finish_stall, finish_swipes) {
            let mut images = evidence.iter().map(Vec::as_slice).collect::<Vec<_>>();
            images.push(&f.png);
            let v=recognition::model(app,r,&images,"这些画面按顺序来自多次上滑前后。只有能确认始终是同一段视频、金币停止、没有加载/网络错误，才选择 finished；能换视频选 continue；无法区分视频播放变化和换视频选 unknown",&["finished","continue","loading","unknown"]).await?;
            if v.choice == "finished" {
                shot(app, r, &f, "complete").await?;
                return Ok(());
            }
            state = Earnings::default();
            evidence.clear();
            previous = None;
            pause(5.).await;
            continue;
        }
        if state.idle >= stall && state.since_swipe >= stall {
            if evidence.len() >= 3 {
                evidence.remove(0);
            }
            evidence.push(f.png.clone());
            r.pending = Some("qishui_swipe".into());
            save(app, r).await?;
            app.adb
                .swipe(&r.device_id, [0.5, 0.83], [0.5, 0.21], 300)
                .await?;
            pause(2.).await;
            let after = recognition::frame(app, &r.device_id).await?;
            let next_identity = after.region_text(identity_region);
            let changed =
                !identity.is_empty() && !next_identity.is_empty() && identity != next_identity;
            if changed {
                state.failures = 0;
                state.idle = 0.;
                evidence.clear();
                shot(app, r, &after, "new-video").await?;
            } else {
                state.failures += 1;
            }
            state.identity = next_identity;
            state.since_swipe = 0.;
            r.pending = None;
            save(app, r).await?;
            app.event(
                r,
                if changed {
                    "已切换视频，继续观察金币"
                } else {
                    "视频标识未变化，继续复查"
                },
            )
            .await?;
            previous = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feed_button_requires_page_and_unique_target_in_region() {
        let mut frame = Frame {
            png: vec![],
            image: image::DynamicImage::new_rgb8(1060, 2376),
            words: vec![
                recognition::Word {
                    text: "招财鸭".into(),
                    x: 527,
                    y: 154,
                    confidence: 1.,
                },
                recognition::Word {
                    text: "喂鸭".into(),
                    x: 528,
                    y: 1866,
                    confidence: 1.,
                },
            ],
        };
        let area = [0.3, 0.68, 0.7, 0.86];
        assert_eq!(
            tap_target(&frame, "喂鸭", "招财鸭", area).unwrap(),
            (528, 1866)
        );
        frame.words[1].y = 300;
        assert!(tap_target(&frame, "喂鸭", "招财鸭", area).is_err());
        frame.words[1].y = 1866;
        frame.words.push(frame.words[1].clone());
        assert!(tap_target(&frame, "喂鸭", "招财鸭", area).is_err());
        frame.words.pop();
        frame.words.remove(0);
        assert!(tap_target(&frame, "喂鸭", "招财鸭", area).is_err());
        let w: Workflow =
            serde_json::from_str(include_str!("../workflows/feed-duck.json")).unwrap();
        samples(&w).unwrap();
        assert!(
            with_tap_settings(
                w.clone(),
                Some(&TapSettings {
                    count: 0,
                    interval: 5.
                })
            )
            .is_err()
        );
        assert!(
            with_tap_settings(
                w.clone(),
                Some(&TapSettings {
                    count: 1,
                    interval: 0.
                })
            )
            .is_err()
        );
        let adjusted = with_tap_settings(
            w,
            Some(&TapSettings {
                count: 20,
                interval: 3.,
            }),
        )
        .unwrap();
        assert!(matches!(
            adjusted.nodes[0].action,
            Action::TapLoop {
                count: 20,
                interval: 3.,
                ..
            }
        ));
    }
    #[test]
    fn swipe_parameters_replace_count_without_changing_template() {
        let original: Workflow =
            serde_json::from_str(include_str!("../workflows/swipe.json")).unwrap();
        let changed = with_swipe_settings(
            original.clone(),
            Some(&SwipeSettings {
                minutes: 15.,
                interval: 12.5,
            }),
        )
        .unwrap();
        assert!(matches!(
            changed.nodes[0].action,
            Action::SwipeLoop {
                interval: 12.5,
                count: None,
                minutes: Some(15.),
                ..
            }
        ));
        assert!(matches!(
            original.nodes[0].action,
            Action::SwipeLoop { count: Some(3), .. }
        ));
        for minutes in [0., -1., f64::NAN, 10081.] {
            assert!(
                with_swipe_settings(
                    original.clone(),
                    Some(&SwipeSettings {
                        minutes,
                        interval: 25.
                    })
                )
                .is_err()
            );
        }
        for interval in [0., -1., f64::INFINITY, 86401.] {
            assert!(
                with_swipe_settings(
                    original.clone(),
                    Some(&SwipeSettings {
                        minutes: 15.,
                        interval
                    })
                )
                .is_err()
            );
        }
    }

    #[test]
    fn known_negative_branch_uses_local_rules() {
        let cases = vec![Branch {
            contains: "任务完成".into(),
            next: "end".into(),
        }];
        assert_eq!(
            branch_choice(&cases, "repeat", Some("视频页"), "视频页 正在播放"),
            Some("repeat")
        );
        assert_eq!(
            branch_choice(&cases, "repeat", Some("视频页"), "其他页面 任务完成"),
            None
        );
        assert_eq!(
            branch_choice(&cases, "repeat", Some("视频页"), "视频页 任务完成"),
            Some("end")
        );
    }
    #[test]
    fn templates_validate() {
        for w in builtins() {
            validate(&w).unwrap();
            samples(&w).unwrap();
        }
    }
    #[test]
    fn dangling_branch_is_rejected() {
        let mut w = builtins().remove(0);
        w.nodes[0].next = Some("absent".into());
        assert!(validate(&w).is_err());
    }
    #[test]
    fn coins_or_motion_reset_completion() {
        let mut e = Earnings {
            last_coin: Some(100),
            idle: 12.,
            failures: 3,
            ..Default::default()
        };
        assert!(e.finished(10., 3));
        e.observe(Some(101), false, 1.);
        assert!(!e.finished(10., 3));
        e.idle = 15.;
        e.failures = 3;
        e.observe(Some(101), true, 1.);
        assert!(!e.finished(10., 3));
    }
    #[test]
    fn three_failed_swipes_and_ten_seconds_required() {
        let mut e = Earnings {
            idle: 9.,
            failures: 3,
            ..Default::default()
        };
        assert!(!e.finished(10., 3));
        e.idle = 10.;
        e.failures = 2;
        assert!(!e.finished(10., 3));
        e.failures = 3;
        assert!(e.finished(10., 3));
    }
}
