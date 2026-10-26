use crate::{
    App,
    drafts::{Draft, DraftNode},
    engine, recognition,
    types::*,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub device_id: String,
    pub width: u32,
    pub height: u32,
    pub display: String,
    pub x: f64,
    pub y: f64,
    pub to_x: f64,
    pub to_y: f64,
}

fn number(c: &Value, key: &str) -> Result<f64> {
    let label = match key {
        "seconds" => "等待时长",
        "count" => "次数",
        "interval" => "间隔",
        "duration_ms" => "滑动时长",
        "timeout" => "最长等待时间",
        _ => key,
    };
    c[key]
        .as_f64()
        .filter(|n| n.is_finite())
        .with_context(|| format!("请填写有效的{label}"))
}
fn count(c: &Value) -> Result<u32> {
    let n = number(c, "count")?;
    ensure!(
        n.fract() == 0. && (1. ..=10000.).contains(&n),
        "次数需为 1 至 10000 的整数"
    );
    Ok(n as u32)
}
fn target<'a>(step: &'a DraftNode, port: &str) -> Result<&'a str> {
    let label = match port {
        "next" => "下一步",
        "body" => "执行本轮",
        "done" => "循环完成",
        "yes" if step.kind == "wait_condition" => "已满足",
        "yes" => "符合",
        "no" => "不符合",
        "timeout" => "已超时",
        _ => port,
    };
    step.targets
        .get(port)
        .map(String::as_str)
        .with_context(|| format!("“{}”尚未连接“{}”出口", step.label, label))
}
fn profiles(step: &DraftNode) -> Result<Vec<Profile>> {
    serde_json::from_value(step.config.get("profiles").cloned().unwrap_or(json!([])))
        .context("手机位置标注格式无效")
}
fn regions(condition: &Value) -> Result<Vec<Profile>> {
    serde_json::from_value(condition.get("regions").cloned().unwrap_or(json!([])))
        .context("识别区域标注格式无效")
}
pub fn needs_text(step: &DraftNode) -> bool {
    step.config["conditions"]
        .as_array()
        .is_some_and(|items| items.iter().any(|c| c["source"] == "text"))
}
fn is_condition(step: &DraftNode) -> bool {
    matches!(step.kind.as_str(), "condition" | "wait_condition")
}
fn canvas_node<'a>(w: &'a Workflow, id: &str) -> Result<&'a DraftNode> {
    match &w
        .nodes
        .iter()
        .find(|n| n.id == id)
        .context("循环不存在")?
        .action
    {
        Action::Canvas { step } => Ok(step),
        _ => bail!("所属步骤不是循环"),
    }
}

pub fn compile(draft: &Draft, version: u32) -> Result<Workflow> {
    crate::drafts::validate(draft)?;
    let mut devices: Option<HashSet<String>> = None;
    for step in &draft.nodes {
        if matches!(step.kind.as_str(), "tap" | "swipe") {
            let supported: HashSet<_> = profiles(step)?.into_iter().map(|p| p.device_id).collect();
            devices = Some(match devices {
                Some(current) => current.intersection(&supported).cloned().collect(),
                None => supported,
            });
        }
        if let Some(conditions) = step.config["conditions"].as_array() {
            for condition in conditions {
                if condition["source"] != "text" {
                    continue;
                }
                let areas = regions(condition)?;
                if !areas.is_empty() {
                    let supported = areas
                        .into_iter()
                        .map(|p| p.device_id)
                        .collect::<HashSet<_>>();
                    devices = Some(match devices {
                        Some(current) => current.intersection(&supported).cloned().collect(),
                        None => supported,
                    });
                }
            }
        }
    }
    if let Some(ids) = &devices {
        ensure!(
            !ids.is_empty(),
            "请先为每个点击和滑动步骤标注至少一台共同适用的手机"
        );
    }
    let nodes = draft
        .nodes
        .iter()
        .map(|step| Node {
            id: step.id.clone(),
            label: step.label.clone(),
            next: None,
            action: if step.kind == "end" {
                Action::End {
                    result: step.config["result"].as_str().unwrap_or("succeeded").into(),
                }
            } else {
                let mut executable = step.clone();
                executable.position = [0., 0.];
                Action::Canvas { step: executable }
            },
        })
        .collect();
    let mut tests = Vec::new();
    for step in &draft.nodes {
        if step.kind == "end" {
            continue;
        }
        if is_condition(step) {
            if let Some(samples) = step.config["samples"].as_array() {
                for sample in samples {
                    tests.push(SampleCase {
                        node: step.id.clone(),
                        text: serde_json::to_string(sample)?,
                        expected: sample["expected"].as_str().unwrap_or("").into(),
                        image: None,
                        after: false,
                        target_region: None,
                    });
                }
            }
        } else {
            tests.push(SampleCase {
                node: step.id.clone(),
                text: String::new(),
                expected: "valid".into(),
                image: None,
                after: false,
                target_region: None,
            });
        }
    }
    let mut devices: Vec<_> = devices.unwrap_or_default().into_iter().collect();
    devices.sort();
    let workflow = Workflow {
        schema_version: 1,
        id: format!("manual-{}", draft.id),
        version,
        name: draft.name.clone(),
        description: draft.description.clone(),
        devices,
        entry: draft.entry.clone(),
        nodes,
        model_limit: 0,
        active: false,
        sample_validated: false,
        tests,
        recognition: RecognitionMode::Local,
    };
    engine::validate(&workflow)?;
    engine::samples(&workflow)?;
    Ok(workflow)
}

pub fn validate(w: &Workflow) -> Result<()> {
    ensure!(
        w.schema_version == 1
            && w.version > 0
            && !w.name.trim().is_empty()
            && w.name.chars().count() <= 80,
        "工作流名称或版本无效"
    );
    ensure!(
        !w.id.is_empty()
            && w.id.len() <= 80
            && w.id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c)),
        "工作流编号无效"
    );
    ensure!(
        w.recognition == RecognitionMode::Local,
        "人工编排条件目前使用本地识别"
    );
    ensure!(
        !w.nodes.is_empty() && w.nodes.len() <= 200,
        "步骤数量需为 1 至 200"
    );
    let ids: HashSet<_> = w.nodes.iter().map(|n| n.id.as_str()).collect();
    ensure!(
        ids.len() == w.nodes.len() && ids.contains(w.entry.as_str()),
        "步骤重复或入口不存在"
    );
    let mut links: HashMap<String, Vec<String>> = HashMap::new();
    for node in &w.nodes {
        ensure!(
            node.next.is_none() && !node.label.trim().is_empty(),
            "编排步骤的去向由出口决定"
        );
        match &node.action {
            Action::End { result } => {
                ensure!(
                    ["succeeded", "needs_help"].contains(&result.as_str()),
                    "结束结果无效"
                );
                links.insert(node.id.clone(), vec![]);
            }
            Action::Canvas { step } => {
                ensure!(
                    step.id == node.id && step.label == node.label,
                    "编排步骤信息不一致"
                );
                let c = &step.config;
                let outgoing: Vec<String> = match step.kind.as_str() {
                    "back" | "home" | "recents" => vec![target(step, "next")?.into()],
                    "wait" => {
                        ensure!(
                            (0. ..=86400.).contains(&number(c, "seconds")?),
                            "等待需为 0 至 86400 秒"
                        );
                        vec![target(step, "next")?.into()]
                    }
                    "tap" | "swipe" | "loop" => {
                        count(c)?;
                        ensure!(
                            (0. ..=86400.).contains(&number(c, "interval")?),
                            "间隔需为 0 至 86400 秒"
                        );
                        if step.kind == "loop" {
                            vec![target(step, "body")?.into(), target(step, "done")?.into()]
                        } else {
                            if step.kind == "swipe" {
                                ensure!(
                                    (100. ..=2000.).contains(&number(c, "duration_ms")?),
                                    "滑动时长需为 100 至 2000 毫秒"
                                );
                            }
                            let locations = profiles(step)?;
                            ensure!(!locations.is_empty(), "{} 尚未按手机标注位置", node.label);
                            let mut seen = HashSet::new();
                            for p in locations {
                                ensure!(
                                    !p.device_id.is_empty()
                                        && p.width > 0
                                        && p.height > 0
                                        && p.width <= 10000
                                        && p.height <= 10000,
                                    "手机位置标注尺寸无效"
                                );
                                ensure!(
                                    [p.x, p.y, p.to_x, p.to_y]
                                        .iter()
                                        .all(|n| n.is_finite() && (0. ..=1.).contains(n)),
                                    "标注位置超出画面"
                                );
                                ensure!(
                                    seen.insert((p.device_id, p.width, p.height, p.display)),
                                    "手机屏幕标注重复"
                                );
                            }
                            vec![target(step, "next")?.into()]
                        }
                    }
                    "condition" | "wait_condition" => {
                        ensure!(
                            matches!(c["mode"].as_str(), Some("all" | "any")),
                            "请选择条件组合方式"
                        );
                        let conditions = c["conditions"].as_array().context("判断条件为空")?;
                        ensure!(
                            !conditions.is_empty() && conditions.len() <= 20,
                            "判断需为 1 至 20 个条件"
                        );
                        for item in conditions {
                            let value = item["value"].as_str().context("条件值无效")?;
                            ensure!(!value.trim().is_empty(), "条件值不能为空");
                            match item["source"].as_str() {
                                Some("text") => {
                                    ensure!(
                                        matches!(
                                            item["operator"].as_str(),
                                            Some("contains" | "not_contains")
                                        ),
                                        "文字判断方式无效"
                                    );
                                    ensure!(
                                        item["guard"]
                                            .as_str()
                                            .is_some_and(|g| !g.trim().is_empty()),
                                        "文字判断需要页面依据"
                                    );
                                    for area in regions(item)? {
                                        ensure!(
                                            area.width > 0
                                                && area.height > 0
                                                && !area.device_id.is_empty()
                                                && [area.x, area.y, area.to_x, area.to_y]
                                                    .iter()
                                                    .all(
                                                        |n| n.is_finite() && (0. ..=1.).contains(n)
                                                    )
                                                && area.x < area.to_x
                                                && area.y < area.to_y,
                                            "文字识别区域无效"
                                        );
                                    }
                                }
                                Some("app") => ensure!(
                                    matches!(item["operator"].as_str(), Some("is" | "is_not"))
                                        && value.contains('.'),
                                    "应用判断需填写有效应用编号"
                                ),
                                Some("round") => ensure!(
                                    step.kind == "condition"
                                        && matches!(
                                            item["operator"].as_str(),
                                            Some("eq" | "gt" | "lt")
                                        )
                                        && value.parse::<u32>().is_ok(),
                                    "循环轮次条件无效"
                                ),
                                _ => bail!("判断条件类型无效"),
                            }
                        }
                        if step.kind == "wait_condition" {
                            ensure!(
                                (0. ..=86400.).contains(&number(c, "timeout")?)
                                    && number(c, "timeout")? > 0.,
                                "最长等待需大于 0，最多 86400 秒"
                            );
                            ensure!(
                                number(c, "interval")? > 0. && number(c, "interval")? <= 86400.,
                                "检查间隔无效"
                            );
                        }
                        vec![
                            target(step, "yes")?.into(),
                            target(
                                step,
                                if step.kind == "condition" {
                                    "no"
                                } else {
                                    "timeout"
                                },
                            )?
                            .into(),
                        ]
                    }
                    "break" | "continue" => {
                        let owner = c["loop"].as_str().context("循环控制缺少所属循环")?;
                        let owner_step = canvas_node(w, owner)?;
                        ensure!(owner_step.kind == "loop", "所属步骤不是循环");
                        vec![if step.kind == "continue" {
                            owner.into()
                        } else {
                            target(owner_step, "done")?.into()
                        }]
                    }
                    _ => bail!("不支持的编排步骤"),
                };
                ensure!(
                    outgoing.iter().all(|id| ids.contains(id.as_str())),
                    "{} 的下一步不存在",
                    node.label
                );
                let allowed: &[&str] = match step.kind.as_str() {
                    "loop" => &["body", "done"],
                    "condition" => &["yes", "no"],
                    "wait_condition" => &["yes", "timeout"],
                    "break" | "continue" => &[],
                    _ => &["next"],
                };
                ensure!(
                    step.targets
                        .keys()
                        .all(|key| allowed.contains(&key.as_str())),
                    "步骤含未定义出口"
                );
                links.insert(node.id.clone(), outgoing);
            }
            _ => bail!("人工编排版本不能混入未适配的旧步骤"),
        }
    }
    let mut membership = HashMap::<String, String>::new();
    for node in &w.nodes {
        let Action::Canvas { step } = &node.action else {
            continue;
        };
        if step.kind != "loop" {
            continue;
        }
        let body = target(step, "body")?;
        let done = target(step, "done")?;
        ensure!(
            body != done && body != node.id,
            "循环的本轮与完成出口不能相同"
        );
        let mut todo = vec![body.to_owned()];
        let mut visited = HashSet::new();
        while let Some(id) = todo.pop() {
            if id == node.id || !visited.insert(id.clone()) {
                continue;
            }
            ensure!(id != done, "循环内部请通过“退出循环”进入完成出口");
            if let Action::Canvas { step: inside } =
                &w.nodes.iter().find(|n| n.id == id).unwrap().action
            {
                ensure!(inside.kind != "loop", "首版不支持嵌套循环");
                ensure!(
                    membership.insert(id.clone(), node.id.clone()).is_none(),
                    "步骤不能同时属于多个循环"
                );
                if matches!(inside.kind.as_str(), "break" | "continue") {
                    ensure!(
                        inside.config["loop"] == node.id,
                        "循环控制节点的所属循环不匹配"
                    );
                    continue;
                }
            }
            todo.extend(links[&id].clone());
        }
    }
    for node in &w.nodes {
        if let Action::Canvas { step } = &node.action {
            let controls = matches!(step.kind.as_str(), "break" | "continue")
                || step.config["conditions"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|c| c["source"] == "round"));
            ensure!(
                !controls || membership.contains_key(&node.id),
                "循环控制或轮次判断必须位于循环内"
            );
        }
        for target in &links[&node.id] {
            if let Some(owner) = membership.get(target) {
                ensure!(
                    node.id == *owner || membership.get(&node.id) == Some(owner),
                    "不能从循环外直接进入循环内部"
                );
            }
        }
    }
    ensure!(!membership.contains_key(&w.entry), "入口不能设在循环内部");
    fn visit(
        id: &str,
        links: &HashMap<String, Vec<String>>,
        membership: &HashMap<String, String>,
        visiting: &mut HashSet<String>,
        reached: &mut HashSet<String>,
    ) -> Result<()> {
        ensure!(!visiting.contains(id), "发现不受循环节点控制的回线");
        if !reached.insert(id.into()) {
            return Ok(());
        }
        visiting.insert(id.into());
        for next in &links[id] {
            if membership.get(id).is_some_and(|owner| owner == next) {
                continue;
            }
            visit(next, links, membership, visiting, reached)?;
        }
        visiting.remove(id);
        Ok(())
    }
    let mut reached = HashSet::new();
    visit(
        &w.entry,
        &links,
        &membership,
        &mut HashSet::new(),
        &mut reached,
    )?;
    ensure!(reached.len() == w.nodes.len(), "存在从入口无法到达的步骤");
    ensure!(
        w.nodes
            .iter()
            .any(|n| reached.contains(&n.id) && matches!(n.action, Action::End { .. })),
        "流程缺少可到达的结束步骤"
    );
    let mut upper = 0u64;
    for node in &w.nodes {
        let cost = match &node.action {
            Action::Canvas { step } if step.kind == "loop" => u64::from(count(&step.config)?) + 1,
            Action::Canvas { step } if matches!(step.kind.as_str(), "tap" | "swipe") => {
                u64::from(count(&step.config)?)
            }
            _ => 1,
        };
        let rounds = if let Some(owner) = membership.get(&node.id) {
            u64::from(count(&canvas_node(w, owner)?.config)?)
        } else {
            1
        };
        upper = upper.saturating_add(cost.saturating_mul(rounds));
    }
    ensure!(upper <= 10000, "预计动作上限超过 10000，请减少重复次数");
    Ok(())
}

fn evaluate(
    step: &DraftNode,
    text: Option<&str>,
    app: Option<&str>,
    round: Option<u64>,
    area_text: Option<&HashMap<usize, String>>,
) -> Option<bool> {
    let values: Vec<Option<bool>> = step.config["conditions"]
        .as_array()?
        .iter()
        .enumerate()
        .map(|(index, c)| {
            let value = c["value"].as_str()?;
            match c["source"].as_str()? {
                "text" => {
                    let observed = text?;
                    if !observed.contains(c["guard"].as_str()?) {
                        return None;
                    }
                    let observed = area_text
                        .and_then(|areas| areas.get(&index))
                        .map(String::as_str)
                        .unwrap_or(observed);
                    Some(if c["operator"] == "not_contains" {
                        !observed.contains(value)
                    } else {
                        observed.contains(value)
                    })
                }
                "app" => app.map(|current| {
                    if c["operator"] == "is_not" {
                        current != value
                    } else {
                        current == value
                    }
                }),
                "round" => {
                    let (current, expected) = (round?, value.parse::<u64>().ok()?);
                    Some(match c["operator"].as_str()? {
                        "gt" => current > expected,
                        "lt" => current < expected,
                        _ => current == expected,
                    })
                }
                _ => None,
            }
        })
        .collect();
    if step.config["mode"] == "any" {
        if values.contains(&Some(true)) {
            Some(true)
        } else if values.contains(&None) {
            None
        } else {
            Some(false)
        }
    } else if values.contains(&Some(false)) {
        Some(false)
    } else if values.contains(&None) {
        None
    } else {
        Some(true)
    }
}
pub fn sample_choice(step: &DraftNode, input: &str) -> Result<String> {
    if !is_condition(step) {
        return Ok("valid".into());
    }
    let sample: Value = serde_json::from_str(input).context("判断样本格式无效")?;
    Ok(match evaluate(
        step,
        sample["text"].as_str(),
        sample["app"].as_str(),
        sample["round"].as_u64(),
        None,
    ) {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
    .into())
}
pub fn require_samples(step: &DraftNode, tests: &[SampleCase]) -> Result<()> {
    if !is_condition(step) {
        return Ok(());
    }
    for expected in ["yes", "no"]
        .into_iter()
        .chain(needs_text(step).then_some("unknown"))
    {
        let label = match expected {
            "yes" => "符合",
            "no" => "不符合",
            _ => "无法确定",
        };
        ensure!(
            tests
                .iter()
                .any(|t| t.node == step.id && t.expected == expected),
            "{} 缺少“{}”样本",
            step.label,
            label
        );
    }
    Ok(())
}

async fn delay(app: &App, r: &mut Run, seconds: f64) -> Result<()> {
    let mut elapsed = r.checkpoint["wait_elapsed"].as_f64().unwrap_or(0.);
    while elapsed < seconds {
        let started = tokio::time::Instant::now();
        let outcome = engine::pause(app, r, (seconds - elapsed).min(0.5)).await;
        elapsed = (elapsed + started.elapsed().as_secs_f64()).min(seconds);
        r.checkpoint["wait_elapsed"] = json!(elapsed);
        engine::save(app, r).await?;
        outcome?;
    }
    Ok(())
}
fn package(activity: &str) -> Option<&str> {
    activity
        .split_whitespace()
        .find_map(|part| part.split_once('/').map(|(p, _)| p.trim_start_matches('{')))
}
async fn observe(app: &App, r: &mut Run, step: &DraftNode, record_negative: bool) -> Result<bool> {
    let activity = app.adb.foreground(&r.device_id).await?;
    let frame = if needs_text(step) {
        recognition::frame(app, &r.device_id).await.ok()
    } else {
        None
    };
    let round = r.canvas_progress["loops"]
        .as_object()
        .and_then(|items| items.values().find(|v| v["phase"] == "body"))
        .and_then(|v| v["round"].as_u64());
    let text = frame
        .as_ref()
        .filter(|frame| frame.words.iter().all(|word| word.confidence >= 0.5))
        .map(|frame| frame.text());
    let mut area_text = HashMap::new();
    if let Some(frame) = &frame {
        for (index, condition) in step.config["conditions"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let areas = regions(condition)?;
            if !areas.is_empty() {
                let screen = app.adb.canvas_screen(&r.device_id).await?;
                let area = areas
                    .iter()
                    .find(|area| {
                        area.device_id == r.device_id
                            && area.width == screen.width
                            && area.height == screen.height
                            && area.display == screen.display
                            && frame.image.width() == screen.width
                            && frame.image.height() == screen.height
                    })
                    .context("当前屏幕没有匹配的文字识别区域标注")?;
                area_text.insert(
                    index,
                    frame.region_text([area.x, area.y, area.to_x, area.to_y]),
                );
            }
        }
    }
    let choice = evaluate(
        step,
        text.as_deref(),
        package(&activity),
        round,
        Some(&area_text),
    );
    if let Some(frame) = &frame {
        if record_negative || choice != Some(false) {
            engine::shot(app, r, frame, "condition").await?;
        }
    }
    choice.context("当前页面或条件无法确认，需要人工处理")
}

pub async fn execute(app: &App, r: &mut Run, step: &DraftNode) -> Result<()> {
    let c = &step.config;
    match step.kind.as_str() {
        "back" | "home" | "recents" => {
            ensure!(r.pending.is_none(), "上次按键结果未确认，不会重复发送");
            let key = match step.kind.as_str() {
                "back" => "KEYCODE_BACK",
                "home" => "KEYCODE_HOME",
                _ => "KEYCODE_APP_SWITCH",
            };
            r.pending = Some("canvas_key".into());
            engine::save(app, r).await?;
            let output = app
                .adb
                .phone(&r.device_id, &["shell", "input", "keyevent", key])
                .await?;
            ensure!(
                String::from_utf8_lossy(&output).trim().is_empty(),
                "手机未确认按键命令，已保留未确认状态"
            );
            r.pending = None;
            r.node = target(step, "next")?.into();
        }
        "wait" => {
            delay(app, r, number(c, "seconds")?).await?;
            r.node = target(step, "next")?.into();
        }
        "tap" | "swipe" => {
            ensure!(r.pending.is_none(), "上次手机动作结果未确认，不会重复执行");
            let mut done = r.checkpoint["done"].as_u64().unwrap_or(0);
            let total = u64::from(count(c)?);
            while done < total {
                engine::ready(app, r).await?;
                if done > 0 {
                    delay(app, r, number(c, "interval")?).await?;
                }
                let screen = app.adb.canvas_screen(&r.device_id).await?;
                let p = profiles(step)?
                    .into_iter()
                    .find(|p| {
                        p.device_id == r.device_id
                            && p.width == screen.width
                            && p.height == screen.height
                            && p.display == screen.display
                    })
                    .context("当前手机或屏幕形态没有匹配标注，请停止任务并重新标注")?;
                let activity = app.adb.foreground(&r.device_id).await?;
                let current = package(&activity).context("未能确认当前应用")?;
                if let Some(expected) = r.checkpoint["foreground"].as_str() {
                    ensure!(expected == current, "应用已改变，已停止重复动作");
                }
                r.checkpoint["foreground"] = json!(current);
                r.pending = Some(format!("canvas_{}", step.kind));
                engine::save(app, r).await?;
                let pixel =
                    |v: f64, n: u32| ((v * f64::from(n)).round() as u32).min(n - 1).to_string();
                let display = screen.input_display.to_string();
                let (x, y) = (pixel(p.x, screen.width), pixel(p.y, screen.height));
                let (to_x, to_y, duration) = if step.kind == "swipe" {
                    (
                        pixel(p.to_x, screen.width),
                        pixel(p.to_y, screen.height),
                        (number(c, "duration_ms")? as u32).to_string(),
                    )
                } else {
                    (String::new(), String::new(), String::new())
                };
                let mut arguments = vec!["shell", "input"];
                if screen.display != "default" {
                    arguments.extend(["-d", display.as_str()]);
                }
                if step.kind == "tap" {
                    arguments.extend(["tap", &x, &y]);
                } else {
                    arguments.extend(["swipe", &x, &y, &to_x, &to_y, &duration]);
                }
                let output = app.adb.phone(&r.device_id, &arguments).await?;
                ensure!(
                    String::from_utf8_lossy(&output).trim().is_empty(),
                    "手机未确认输入命令，已保留未确认状态"
                );
                done += 1;
                r.pending = None;
                r.checkpoint["done"] = json!(done);
                r.checkpoint["wait_elapsed"] = json!(0);
                engine::save(app, r).await?;
                app.event(r, &format!("{}：已执行 {done}/{total} 次", step.label))
                    .await?;
            }
            r.node = target(step, "next")?.into();
        }
        "loop" => {
            if !r.canvas_progress.is_object() {
                r.canvas_progress = json!({});
            }
            if !r.canvas_progress["loops"].is_object() {
                r.canvas_progress["loops"] = json!({});
            }
            if r.canvas_progress["loops"].get(&step.id).is_none() {
                r.canvas_progress["loops"][&step.id] =
                    json!({"ended":0,"round":1,"phase":"between"});
            }
            let state = &mut r.canvas_progress["loops"][&step.id];
            ensure!(state["phase"] != "done", "循环已经结束，不能从外部再次进入");
            if state["phase"] == "body" {
                state["ended"] = json!(state["ended"].as_u64().unwrap_or(0) + 1);
                state["phase"] = json!("between");
            }
            let ended = state["ended"].as_u64().unwrap_or(0);
            engine::save(app, r).await?;
            if ended >= u64::from(count(c)?) {
                r.canvas_progress["loops"][&step.id]["phase"] = json!("done");
                r.node = target(step, "done")?.into();
            } else {
                if ended > 0 {
                    delay(app, r, number(c, "interval")?).await?;
                }
                r.canvas_progress["loops"][&step.id]["phase"] = json!("body");
                r.canvas_progress["loops"][&step.id]["round"] = json!(ended + 1);
                r.node = target(step, "body")?.into();
                app.event(r, &format!("循环：第 {} / {} 轮", ended + 1, count(c)?))
                    .await?;
            }
        }
        "break" | "continue" => {
            let owner = c["loop"].as_str().context("缺少所属循环")?;
            ensure!(
                r.canvas_progress["loops"][owner]["phase"] == "body",
                "当前不在所属循环内"
            );
            if step.kind == "break" {
                r.node = target(canvas_node(&r.workflow, owner)?, "done")?.into();
                r.canvas_progress["loops"][owner]["phase"] = json!("done");
            } else {
                r.node = owner.into();
            }
        }
        "condition" => {
            let pass = observe(app, r, step, true).await?;
            r.node = target(step, if pass { "yes" } else { "no" })?.into();
        }
        "wait_condition" => {
            let timeout = number(c, "timeout")?;
            loop {
                engine::ready(app, r).await?;
                let started = tokio::time::Instant::now();
                let pass = observe(app, r, step, false).await?;
                let elapsed = r.checkpoint["condition_elapsed"].as_f64().unwrap_or(0.)
                    + started.elapsed().as_secs_f64();
                r.checkpoint["condition_elapsed"] = json!(elapsed);
                engine::save(app, r).await?;
                if pass {
                    r.node = target(step, "yes")?.into();
                    break;
                }
                if elapsed >= timeout {
                    if let Ok(frame) = recognition::capture(app, &r.device_id).await {
                        engine::shot(app, r, &frame, "condition-timeout").await?;
                    }
                    r.node = target(step, "timeout")?.into();
                    break;
                }
                let wait = number(c, "interval")?.min(timeout - elapsed);
                let clock = tokio::time::Instant::now();
                let result = engine::pause(app, r, wait.min(0.5)).await;
                r.checkpoint["condition_elapsed"] = json!(elapsed + clock.elapsed().as_secs_f64());
                engine::save(app, r).await?;
                result?;
                let mut remaining = (wait - clock.elapsed().as_secs_f64()).max(0.);
                while remaining > 0. {
                    let clock = tokio::time::Instant::now();
                    let result = engine::pause(app, r, remaining.min(0.5)).await;
                    let spent = clock.elapsed().as_secs_f64();
                    remaining = (remaining - spent).max(0.);
                    r.checkpoint["condition_elapsed"] =
                        json!(r.checkpoint["condition_elapsed"].as_f64().unwrap_or(0.) + spent);
                    engine::save(app, r).await?;
                    result?;
                }
            }
        }
        _ => bail!("步骤尚未适配执行"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_page_is_not_a_negative_match() {
        let step: DraftNode = serde_json::from_value(json!({"id":"a","kind":"condition","label":"判断","position":[0,0],"targets":{},"config":{"mode":"all","conditions":[{"source":"text","operator":"not_contains","value":"领取","guard":"任务页"}]}})).unwrap();
        assert_eq!(evaluate(&step, Some("陌生页"), None, None, None), None);
        assert_eq!(
            evaluate(&step, Some("任务页"), None, None, None),
            Some(true)
        );
        assert_eq!(
            evaluate(&step, Some("任务页 领取"), None, None, None),
            Some(false)
        );
    }
    #[test]
    fn combined_conditions_keep_known_results() {
        let mut step: DraftNode = serde_json::from_value(json!({"id":"a","kind":"condition","label":"判断","position":[0,0],"targets":{},"config":{"mode":"all","conditions":[{"source":"text","operator":"contains","value":"领取","guard":"任务页"},{"source":"app","operator":"is","value":"com.example.app","guard":""}]}})).unwrap();
        assert_eq!(
            evaluate(&step, None, Some("other.app"), None, None),
            Some(false)
        );
        step.config["mode"] = json!("any");
        assert_eq!(
            evaluate(&step, None, Some("com.example.app"), None, None),
            Some(true)
        );
        assert_eq!(evaluate(&step, None, Some("other.app"), None, None), None);
    }
}
