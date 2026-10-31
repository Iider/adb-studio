use crate::{
    App, engine,
    types::{Action, Node, Run, Workflow},
    workflow_inputs,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    #[serde(default, skip_serializing_if = "Mode::is_visit")]
    pub mode: Mode,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Visit,
    Run,
}
impl Mode {
    fn is_visit(&self) -> bool {
        *self == Self::Visit
    }
}
pub type Fields = BTreeMap<String, BTreeMap<String, Range>>;
pub type Samples = BTreeMap<String, BTreeMap<String, f64>>;

pub fn is_spec(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 1
            && object.get("random").is_some_and(|range| {
                range.as_object().is_some_and(|fields| {
                    (2..=3).contains(&fields.len())
                        && fields
                            .keys()
                            .all(|key| ["min", "max", "mode"].contains(&key.as_str()))
                        && fields
                            .get("mode")
                            .is_none_or(|mode| matches!(mode.as_str(), Some("visit" | "run")))
                        && ["min", "max"].iter().all(|key| {
                            fields.get(*key).is_some_and(|n| {
                                n.is_number()
                                    || n.is_null()
                                    || workflow_inputs::reference(n).is_some()
                            })
                        })
                })
            })
    })
}

impl Range {
    fn bounds(&self, kind: &str, key: &str) -> Result<(u32, u32, f64)> {
        ensure!(
            workflow_inputs::field_allowed(kind, key),
            "此字段不支持随机值"
        );
        ensure!(
            self.min.is_finite() && self.max.is_finite() && self.min <= self.max,
            "随机最小值不能大于最大值"
        );
        if key == "count" {
            ensure!(
                self.min.fract() == 0.
                    && self.max.fract() == 0.
                    && self.min >= 1.
                    && self.max <= 10000.,
                "随机次数需为 1 至 10000 的整数"
            );
        } else if key == "duration_ms" {
            ensure!(
                self.min.fract() == 0.
                    && self.max.fract() == 0.
                    && self.min >= 100.
                    && self.max <= 2000.,
                "随机滑动时长需为 100 至 2000 的整数毫秒"
            );
        } else {
            ensure!(
                self.min >= 0. && self.max <= 86400. && (kind != "wait_condition" || self.min > 0.),
                "随机时间区间超出此字段允许的范围"
            );
        }
        let scale = if matches!(key, "count" | "duration_ms") {
            1.
        } else {
            1000.
        };
        let low = (self.min * scale - 0.000001).ceil().max(0.) as u32;
        let high = (self.max * scale + 0.000001).floor().max(0.) as u32;
        ensure!(low <= high, "随机区间内没有有效数值，时间精度为 0.001 秒");
        Ok((low, high, scale))
    }

    pub fn upper(&self, kind: &str, key: &str) -> Result<f64> {
        let (_, high, scale) = self.bounds(kind, key)?;
        Ok(f64::from(high) / scale)
    }

    fn sample(&self, kind: &str, key: &str) -> Result<f64> {
        let (low, high, scale) = self.bounds(kind, key)?;
        let width = u64::from(high - low) + 1;
        let population = u64::from(u32::MAX) + 1;
        let limit = population - population % width;
        loop {
            // 复用已有 UUID 随机源；前四字节不包含版本位，拒绝采样避免取模偏差。
            let bytes = uuid::Uuid::new_v4();
            let value = u64::from(u32::from_le_bytes(
                bytes.as_bytes()[..4].try_into().unwrap(),
            ));
            if value < limit {
                return Ok((f64::from(low) + (value % width) as f64) / scale);
            }
        }
    }
}

pub fn resolve(
    value: &Value,
    kind: &str,
    key: &str,
    inputs: &[workflow_inputs::Input],
    values: &workflow_inputs::Values,
) -> Result<Option<Range>> {
    if !value
        .as_object()
        .is_some_and(|object| object.contains_key("random"))
    {
        return Ok(None);
    }
    ensure!(is_spec(value), "随机区间格式无效");
    let mut resolved = value["random"].clone();
    for bound in ["min", "max"] {
        if let Some(id) = workflow_inputs::reference(&value["random"][bound]) {
            let input = inputs
                .iter()
                .find(|input| input.id == id)
                .context("随机区间引用的变量已不存在")?;
            ensure!(
                key != "count" || input.kind == "integer",
                "随机次数的区间需使用整数变量"
            );
            resolved[bound] = json!(values.get(id).context("随机区间变量缺少数值")?);
        }
    }
    let range: Range =
        serde_json::from_value(resolved).context("请填写随机区间的最小值和最大值")?;
    range.bounds(kind, key)?;
    Ok(Some(range))
}

pub fn fields(workflow: &Workflow, values: &workflow_inputs::Values) -> Result<Fields> {
    let mut result = Fields::new();
    for node in &workflow.nodes {
        if let Action::Canvas { step } = &node.action {
            for (key, value) in step.config.as_object().context("步骤设置无效")? {
                if let Some(range) = resolve(value, &step.kind, key, &workflow.inputs, values)? {
                    result
                        .entry(node.id.clone())
                        .or_default()
                        .insert(key.clone(), range);
                }
            }
        }
    }
    Ok(result)
}

pub async fn prepare(app: &App, run: &mut Run, node: &mut Node) -> Result<()> {
    let Some(fields) = run.random_fields.get(&node.id).cloned() else {
        return Ok(());
    };
    if run.checkpoint["random_resolved"] == true {
        return Ok(());
    }
    let Action::Canvas { step } = &mut node.action else {
        return Ok(());
    };
    if step.kind == "loop" && !run.canvas_progress["loops"][&node.id].is_null() {
        return Ok(());
    }
    let mut messages = Vec::new();
    for (key, range) in fields {
        let previous = run
            .random_samples
            .get(&node.id)
            .and_then(|items| items.get(&key))
            .copied();
        let reused = range.mode == Mode::Run && previous.is_some();
        let value = if reused {
            previous.unwrap()
        } else {
            range.sample(&step.kind, &key)?
        };
        if range.mode == Mode::Run {
            run.random_samples
                .entry(node.id.clone())
                .or_default()
                .insert(key.clone(), value);
        }
        step.config[&key] = json!(value);
        let label = match key.as_str() {
            "count" if step.kind == "loop" => "轮数",
            "count" => "次数",
            "seconds" => "等待秒数",
            "duration_ms" => "滑动毫秒",
            "interval" => "间隔秒数",
            "after_wait" => "结束后等待秒数",
            "timeout" => "最长等待秒数",
            _ => &key,
        };
        let mode = if reused {
            "沿用本次任务"
        } else if range.mode == Mode::Run {
            "本次任务固定"
        } else {
            "每次进入重抽"
        };
        messages.push(format!("{label} {value}（{mode}）"));
    }
    *run.workflow
        .nodes
        .iter_mut()
        .find(|item| item.id == node.id)
        .context("随机步骤不存在")? = node.clone();
    run.checkpoint["random_resolved"] = json!(true);
    engine::save(app, run).await?;
    app.event(run, &format!("随机取值：{}", messages.join("；")))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(value: &Value, kind: &str, key: &str) -> Result<Option<Range>> {
        resolve(value, kind, key, &[], &workflow_inputs::Values::new())
    }

    #[test]
    fn old_ranges_default_to_visit_and_reject_unknown_modes() {
        let old: Range = serde_json::from_value(json!({"min":2,"max":4})).unwrap();
        assert_eq!(old.mode, Mode::Visit);
        assert_eq!(
            serde_json::to_value(old).unwrap(),
            json!({"min":2.0,"max":4.0})
        );
        assert!(
            parse(
                &json!({"random":{"min":1,"max":2,"mode":"invalid"}}),
                "tap",
                "count"
            )
            .is_err()
        );
        assert_eq!(
            parse(
                &json!({"random":{"min":1,"max":2,"mode":"run"}}),
                "tap",
                "count"
            )
            .unwrap()
            .unwrap()
            .mode,
            Mode::Run
        );
    }

    #[test]
    fn variable_bounds_use_values_and_validate_type_order_and_precision() {
        let id = "00000000-0000-4000-8000-000000000001";
        let mut inputs: Vec<workflow_inputs::Input> = serde_json::from_value(json!([
            {"id":id,"name":"最小次数","type":"integer","default":2}
        ]))
        .unwrap();
        let spec = json!({"random":{"min":{"input":id},"max":5,"mode":"run"}});
        let range = resolve(
            &spec,
            "tap",
            "count",
            &inputs,
            &workflow_inputs::Values::from([(id.into(), 3.)]),
        )
        .unwrap()
        .unwrap();
        assert_eq!((range.min, range.max, range.mode), (3., 5., Mode::Run));
        assert!(
            resolve(
                &spec,
                "tap",
                "count",
                &inputs,
                &workflow_inputs::Values::from([(id.into(), 6.)])
            )
            .is_err()
        );
        assert!(resolve(&spec, "tap", "count", &[], &workflow_inputs::Values::new()).is_err());
        inputs[0].kind = "number".into();
        assert!(
            resolve(
                &spec,
                "tap",
                "count",
                &inputs,
                &workflow_inputs::Values::from([(id.into(), 2.)])
            )
            .is_err()
        );
        let spec = json!({"random":{"min":{"input":id},"max":101}});
        assert!(
            resolve(
                &spec,
                "swipe",
                "duration_ms",
                &inputs,
                &workflow_inputs::Values::from([(id.into(), 100.5)])
            )
            .is_err()
        );
    }

    #[test]
    fn ranges_validate_endpoints_precision_and_equal_bounds() {
        for value in [
            json!({"random":{"min":4,"max":2}}),
            json!({"random":{"min":0,"max":2}}),
            json!({"random":{"min":1.2,"max":2}}),
        ] {
            assert!(parse(&value, "tap", "count").is_err());
        }
        assert!(parse(&json!({"random":{"min":null,"max":2}}), "wait", "seconds").is_err());
        assert!(
            parse(
                &json!({"random":{"min":0.0001,"max":0.0002}}),
                "wait",
                "seconds"
            )
            .is_err()
        );
        for (kind, key, min, max) in [
            ("tap", "count", 1., 3.),
            ("swipe", "duration_ms", 100., 103.),
            ("wait", "seconds", 1.001, 1.005),
            ("wait", "seconds", 1.001, 1.001),
        ] {
            let range = Range {
                min,
                max,
                mode: Mode::Visit,
            };
            for _ in 0..100 {
                let n = range.sample(kind, key).unwrap();
                assert!((min..=max).contains(&n));
                if key != "seconds" {
                    assert_eq!(n.fract(), 0.);
                }
            }
            assert_eq!(range.upper(kind, key).unwrap(), max);
        }
    }
}
