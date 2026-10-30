use crate::{
    canvas,
    drafts::{Draft, DraftNode},
    types::Workflow,
    workflow_inputs::{self, Values},
};
use anyhow::{Context, Result};
use serde_json::json;
use std::collections::{HashMap, HashSet};

// 临时支撑节点只用于复用完整的参数、循环与变量校验，执行器不会进入它们。
pub fn workflow(draft: &Draft, node_id: &str, overrides: &Values) -> Result<Workflow> {
    crate::drafts::validate(draft)?;
    let mut selected = draft
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .context("测试步骤不存在")?
        .clone();
    let used: HashSet<_> = selected
        .config
        .as_object()
        .into_iter()
        .flat_map(|c| c.values())
        .filter_map(workflow_inputs::reference)
        .collect();
    let mut inputs: Vec<_> = draft
        .inputs
        .iter()
        .filter(|input| used.contains(input.id.as_str()))
        .cloned()
        .collect();
    anyhow::ensure!(
        overrides.keys().all(|id| used.contains(id.as_str())),
        "本次输入包含此步骤未使用的变量"
    );
    for input in &mut inputs {
        if input.default.is_none() {
            input.default = overrides.get(&input.id).copied();
        }
    }
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let end = format!("test_end_{suffix}");
    let control = format!("test_control_{suffix}");
    let owner = format!("test_loop_{suffix}");
    let mut nodes = vec![DraftNode {
        id: end.clone(),
        kind: "end".into(),
        label: "测试结束".into(),
        config: json!({"result":"succeeded"}),
        targets: HashMap::new(),
        position: [0., 0.],
    }];
    let needs_owner = matches!(selected.kind.as_str(), "break" | "continue")
        || selected.config["conditions"]
            .as_array()
            .is_some_and(|items| items.iter().any(|c| c["source"] == "round"));
    let next = if needs_owner { &control } else { &end };
    selected.targets = match selected.kind.as_str() {
        "loop" => HashMap::from([
            ("body".into(), control.clone()),
            ("done".into(), end.clone()),
        ]),
        "condition" => HashMap::from([("yes".into(), next.clone()), ("no".into(), next.clone())]),
        "wait_condition" => {
            HashMap::from([("yes".into(), end.clone()), ("timeout".into(), end.clone())])
        }
        "break" | "continue" | "end" => HashMap::new(),
        _ => HashMap::from([("next".into(), end.clone())]),
    };
    if selected.kind == "loop" || needs_owner {
        let loop_id = if needs_owner {
            owner.clone()
        } else {
            selected.id.clone()
        };
        nodes.push(DraftNode {
            id: control,
            kind: "continue".into(),
            label: "测试支撑".into(),
            config: json!({"loop":loop_id}),
            targets: HashMap::new(),
            position: [0., 0.],
        });
        if needs_owner {
            nodes.push(DraftNode {
                id: owner.clone(),
                kind: "loop".into(),
                label: "测试轮次".into(),
                config: json!({"count":1,"interval":0}),
                targets: HashMap::from([
                    ("body".into(), selected.id.clone()),
                    ("done".into(), end),
                ]),
                position: [0., 0.],
            });
            if matches!(selected.kind.as_str(), "break" | "continue") {
                selected.config["loop"] = json!(owner);
            }
        }
    }
    let mut isolated = draft.clone();
    isolated.inputs = inputs;
    isolated.name = format!("单步：{}", selected.label)
        .chars()
        .take(80)
        .collect();
    isolated.entry = if needs_owner {
        owner
    } else {
        selected.id.clone()
    };
    nodes.push(selected);
    isolated.nodes = nodes;
    let workflow = canvas::compile_test(&isolated)?;
    let (resolved, _) = workflow_inputs::resolve(&workflow, overrides)?;
    canvas::validate_test(&resolved)?;
    Ok(workflow)
}
