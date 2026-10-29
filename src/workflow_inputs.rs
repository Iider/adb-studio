use crate::types::{Action, Workflow};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub type Values = BTreeMap<String, f64>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub alias: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub default: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

pub fn reference(value: &Value) -> Option<&str> {
    let object = value.as_object()?;
    (object.len() == 1).then_some(())?;
    object.get("input")?.as_str()
}

pub fn field_allowed(kind: &str, key: &str) -> bool {
    match kind {
        "loop" => matches!(key, "count" | "interval"),
        "tap" => matches!(key, "count" | "interval" | "after_wait"),
        "swipe" => matches!(key, "count" | "interval" | "after_wait" | "duration_ms"),
        "wait" => key == "seconds",
        "wait_condition" => matches!(key, "timeout" | "interval"),
        _ => false,
    }
}

pub fn validate_definitions(inputs: &[Input], complete: bool) -> Result<()> {
    ensure!(inputs.len() <= 20, "最多添加 20 个运行变量");
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for input in inputs {
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok() && ids.insert(&input.id),
            "变量编号无效或重复"
        );
        ensure!(
            input.name.chars().count() <= 40 && input.description.chars().count() <= 200,
            "变量名称最多 40 字，说明最多 200 字"
        );
        ensure!(input.alias.chars().count() <= 40, "显示别名最多 40 字");
        ensure!(
            matches!(input.kind.as_str(), "integer" | "number"),
            "变量类型无效"
        );
        ensure!(
            matches!(input.unit.as_str(), "" | "seconds" | "milliseconds"),
            "变量单位无效"
        );
        ensure!(
            [input.default, input.min, input.max]
                .into_iter()
                .flatten()
                .all(f64::is_finite),
            "变量数值无效"
        );
        if complete {
            ensure!(
                !input.name.trim().is_empty() && names.insert(input.name.trim()),
                "变量名称不能为空或重复"
            );
            ensure!(
                !matches!((input.min, input.max), (Some(min), Some(max)) if min > max),
                "{}：最小值不能大于最大值",
                input.label()
            );
            input.check(
                input
                    .default
                    .with_context(|| format!("{}：请填写默认值", input.label()))?,
            )?;
        }
    }
    Ok(())
}

impl Input {
    pub fn label(&self) -> &str {
        let alias = self.alias.trim();
        if alias.is_empty() {
            self.name.trim()
        } else {
            alias
        }
    }

    fn check(&self, value: f64) -> Result<()> {
        ensure!(
            value.is_finite() && (self.kind != "integer" || value.fract() == 0.),
            "{}：请填写有效的{}",
            self.label(),
            if self.kind == "integer" {
                "整数"
            } else {
                "数字"
            }
        );
        ensure!(
            self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value <= max),
            "{}：数值超出变量设置的范围",
            self.label()
        );
        Ok(())
    }
}

pub fn resolve(workflow: &Workflow, overrides: &Values) -> Result<(Workflow, Values)> {
    validate_definitions(&workflow.inputs, true)?;
    ensure!(
        overrides
            .keys()
            .all(|id| workflow.inputs.iter().any(|input| &input.id == id)),
        "本次输入含不存在的变量，请重新打开运行设置"
    );
    let mut values = Values::new();
    for input in &workflow.inputs {
        let value = overrides
            .get(&input.id)
            .copied()
            .unwrap_or(input.default.unwrap());
        input.check(value)?;
        values.insert(input.id.clone(), value);
    }
    let mut resolved = workflow.clone();
    for node in &mut resolved.nodes {
        let Action::Canvas { step } = &mut node.action else {
            continue;
        };
        let config = step.config.as_object_mut().context("步骤设置无效")?;
        for (key, value) in config.iter_mut() {
            let Some(id) = reference(value) else { continue };
            ensure!(
                field_allowed(&step.kind, key),
                "{}：此字段不支持变量：{key}",
                step.label
            );
            let input = workflow
                .inputs
                .iter()
                .find(|input| input.id == id)
                .with_context(|| format!("{}：引用的变量已不存在", step.label))?;
            let number = values[id];
            let label = match key.as_str() {
                "count" if step.kind == "loop" => "重复轮数",
                "count" => "重复次数",
                "duration_ms" => "滑动时长（100 至 2000 毫秒）",
                "interval" => "间隔时间",
                "timeout" => "最长等待时间",
                "after_wait" => "结束后等待时间",
                _ => "等待时间",
            };
            let unit = if key == "duration_ms" {
                "milliseconds"
            } else if key == "count" {
                ""
            } else {
                "seconds"
            };
            ensure!(
                input.unit.is_empty() || input.unit == unit,
                "{}：变量“{}”的单位不适用于{label}",
                step.label,
                input.label()
            );
            let valid = if key == "count" {
                input.kind == "integer" && number.fract() == 0. && (1. ..=10000.).contains(&number)
            } else if key == "duration_ms" {
                (100. ..=2000.).contains(&number)
            } else {
                (0. ..=86400.).contains(&number) && (step.kind != "wait_condition" || number > 0.)
            };
            ensure!(
                valid,
                "{}：变量“{}”不符合{label}的范围或类型要求",
                step.label,
                input.label()
            );
            *value = serde_json::json!(number);
        }
    }
    Ok((resolved, values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{canvas, drafts::Draft};
    use serde_json::json;

    fn draft() -> Draft {
        serde_json::from_value(json!({
            "id": uuid::Uuid::new_v4().to_string(), "revision":0, "name":"变量", "description":"", "entry":"wait", "viewport":[0,0,1],
            "inputs":[{"id":"00000000-0000-4000-8000-000000000001","name":"等待时间","type":"number","default":2,"min":0,"max":10,"unit":"seconds"}],
            "nodes":[
                {"id":"wait","kind":"wait","label":"等待","config":{"seconds":{"input":"00000000-0000-4000-8000-000000000001"}},"targets":{"next":"end"},"position":[0,0]},
                {"id":"end","kind":"end","label":"结束","config":{"result":"succeeded"},"targets":{},"position":[0,0]}
            ]
        })).unwrap()
    }
    #[test]
    fn aliases_are_optional_preserved_and_do_not_change_references() {
        let mut draft = draft();
        assert_eq!(draft.inputs[0].label(), "等待时间");
        let id = draft.inputs[0].id.clone();
        draft.inputs[0].name = "delay".into();
        draft.inputs[0].alias = "  点击间隔  ".into();
        let workflow = canvas::compile(&draft).unwrap();
        assert_eq!(workflow.inputs[0].label(), "点击间隔");
        let (resolved, values) = resolve(&workflow, &Values::from([(id.clone(), 0.)])).unwrap();
        assert_eq!(values[&id], 0.);
        assert_eq!(resolved.inputs[0].alias, "  点击间隔  ");
        let error = resolve(&workflow, &Values::from([(id.clone(), -1.)]))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("点击间隔："));
        draft.inputs[0].alias = " ".into();
        assert_eq!(draft.inputs[0].label(), "delay");
        draft.inputs[0].alias = "字".repeat(41);
        assert!(validate_definitions(&draft.inputs, false).is_err());
    }

    #[test]
    fn overrides_are_isolated_and_zero_is_not_a_default() {
        let draft = draft();
        let workflow = canvas::compile(&draft).unwrap();
        assert_eq!(workflow.schema_version, 2);
        let id = draft.inputs[0].id.clone();
        let (resolved, values) = resolve(&workflow, &Values::from([(id.clone(), 0.)])).unwrap();
        assert_eq!(values[&id], 0.);
        let Action::Canvas { step } = &resolved.nodes[0].action else {
            panic!()
        };
        assert_eq!(step.config["seconds"], json!(0.));
        let Action::Canvas { step } = &workflow.nodes[0].action else {
            panic!()
        };
        assert!(reference(&step.config["seconds"]).is_some());
        assert_eq!(resolve(&workflow, &Values::new()).unwrap().1[&id], 2.);
    }
    #[test]
    fn invalid_values_references_and_defaults_are_rejected() {
        let mut draft = draft();
        let workflow = canvas::compile(&draft).unwrap();
        let id = draft.inputs[0].id.clone();
        for value in [-1., 11., f64::NAN, f64::INFINITY] {
            assert!(resolve(&workflow, &Values::from([(id.clone(), value)])).is_err());
        }
        assert!(resolve(&workflow, &Values::from([("missing".into(), 1.)])).is_err());
        draft.inputs[0].kind = "integer".into();
        let integer = canvas::compile(&draft).unwrap();
        assert!(resolve(&integer, &Values::from([(id, 1.5)])).is_err());
        draft.inputs[0].default = None;
        crate::drafts::validate(&draft).unwrap();
        assert!(canvas::compile(&draft).is_err());
        draft.inputs.clear();
        assert!(canvas::compile(&draft).is_err());
    }
    #[test]
    fn shared_reference_checks_each_field_and_keeps_unit_rules() {
        let mut draft = draft();
        draft.nodes[0].kind = "wait_condition".into();
        draft.nodes[0].config = json!({"interval":{"input":draft.inputs[0].id}, "timeout":2});
        let workflow = Workflow {
            schema_version: 2,
            id: "test".into(),
            name: "test".into(),
            description: String::new(),
            inputs: draft.inputs.clone(),
            devices: vec![],
            entry: "wait".into(),
            nodes: vec![crate::types::Node {
                id: "wait".into(),
                label: "wait".into(),
                next: None,
                action: Action::Canvas {
                    step: draft.nodes[0].clone(),
                },
            }],
            model_limit: 0,
            tests: vec![],
            recognition: Default::default(),
        };
        assert!(resolve(&workflow, &Values::from([(draft.inputs[0].id.clone(), 0.)])).is_err());
        let mut wrong_unit = workflow.clone();
        wrong_unit.inputs[0].unit = "milliseconds".into();
        assert!(resolve(&wrong_unit, &Values::new()).is_err());
    }
}
