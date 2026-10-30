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
}
pub type Fields = BTreeMap<String, BTreeMap<String, Range>>;

pub fn is_spec(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 1
            && object.get("random").is_some_and(|range| {
                range.as_object().is_some_and(|fields| {
                    fields.len() == 2
                        && ["min", "max"].iter().all(|key| {
                            fields
                                .get(*key)
                                .is_some_and(|n| n.is_number() || n.is_null())
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

pub fn parse(value: &Value, kind: &str, key: &str) -> Result<Option<Range>> {
    if !value
        .as_object()
        .is_some_and(|object| object.contains_key("random"))
    {
        return Ok(None);
    }
    ensure!(is_spec(value), "随机区间格式无效");
    let range: Range = serde_json::from_value(value["random"].clone())
        .context("请填写随机区间的最小值和最大值")?;
    range.bounds(kind, key)?;
    Ok(Some(range))
}

pub fn fields(workflow: &Workflow) -> Result<Fields> {
    let mut result = Fields::new();
    for node in &workflow.nodes {
        if let Action::Canvas { step } = &node.action {
            for (key, value) in step.config.as_object().context("步骤设置无效")? {
                if let Some(range) = parse(value, &step.kind, key)? {
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
        let value = range.sample(&step.kind, &key)?;
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
        messages.push(format!("{label} {value}"));
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
            let range = Range { min, max };
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
