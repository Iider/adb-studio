use crate::{App, types::*};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Word {
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub confidence: f64,
}
pub struct Frame {
    pub png: Vec<u8>,
    pub image: DynamicImage,
    pub words: Vec<Word>,
}
impl Frame {
    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub fn region_text(&self, r: [f64; 4]) -> String {
        let (w, h) = self.image.dimensions();
        self.words
            .iter()
            .filter(|v| {
                v.x as f64 >= r[0] * w as f64
                    && v.x as f64 <= r[2] * w as f64
                    && v.y as f64 >= r[1] * h as f64
                    && v.y as f64 <= r[3] * h as f64
            })
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
pub async fn ocr(app: &App, png: &[u8]) -> Result<Vec<Word>> {
    let mut command = if let Some(bin) = crate::platform::local_ocr(&app.root, &app.data) {
        crate::platform::command(bin)
    } else {
        anyhow::ensure!(
            cfg!(target_os = "macos"),
            "当前平台没有本地文字识别，请使用纯视觉任务，或配置兼容的 bin/ocr"
        );
        let mut c = Command::new("swift");
        c.arg(app.root.join("scripts/ocr.swift"));
        c
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    child
        .stdin
        .take()
        .context("文字识别输入不可用")?
        .write_all(png)
        .await?;
    let out = tokio::time::timeout(Duration::from_secs(12), child.wait_with_output()).await??;
    if !out.status.success() {
        bail!("本机文字识别失败")
    };
    Ok(serde_json::from_slice(&out.stdout)?)
}
pub async fn capture(app: &App, id: &str) -> Result<Frame> {
    let shot = app.adb.capture(id).await?;
    Ok(Frame {
        png: shot.png,
        image: shot.image,
        words: vec![],
    })
}
pub async fn frame(app: &App, id: &str) -> Result<Frame> {
    let mut frame = capture(app, id).await?;
    frame.words = ocr(app, &frame.png).await?;
    Ok(frame)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Verdict {
    pub choice: String,
    pub reason: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub target: Option<[u32; 2]>,
    #[serde(default)]
    pub coin: Option<u64>,
}
fn key(app: &App, name: &str) -> Result<String> {
    if let Ok(v) = std::env::var(name) {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    for file in [app.root.join(".env"), app.root.join("../.env")] {
        if let Ok(s) = std::fs::read_to_string(file) {
            for line in s.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    if k.trim() == name {
                        return Ok(v.trim().trim_matches(['\'', '"']).to_owned());
                    }
                }
            }
        }
    }
    bail!("模型密钥未配置")
}
pub async fn settings(app: &App) -> Result<ModelSettings> {
    Ok(app.db.get("settings", "model").await?.unwrap_or_default())
}

pub async fn request(
    app: &App,
    settings: &ModelSettings,
    images: &[&[u8]],
    prompt: &str,
) -> Result<Verdict> {
    let secret = key(app, &settings.key_env)?;
    let content: Vec<Value> = if settings.api == "responses" {
        images.iter().map(|b|json!({"type":"input_image","image_url":format!("data:image/png;base64,{}",STANDARD.encode(b)),"detail":"high"})).chain(std::iter::once(json!({"type":"input_text","text":prompt}))).collect()
    } else {
        images.iter().map(|b|json!({"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}",STANDARD.encode(b))}})).chain(std::iter::once(json!({"type":"text","text":prompt}))).collect()
    };
    let mut payload = if settings.api == "responses" {
        json!({"model":settings.model,"max_output_tokens":512,"input":[{"role":"user","content":content}]})
    } else {
        json!({"model":settings.model,"max_tokens":512,"messages":[{"role":"user","content":content}]})
    };
    if settings.endpoint.starts_with("https://ark.") {
        payload["thinking"] = json!({"type":"disabled"});
    }
    let response = async {
        let mut response = app
            .model_client
            .post(&settings.endpoint)
            .bearer_auth(secret)
            .json(&payload)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("模型连接失败"))?;
        anyhow::ensure!(
            response.status().is_success(),
            "模型请求失败：HTTP {}",
            response.status().as_u16()
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("模型响应中断"))?
        {
            anyhow::ensure!(bytes.len() + chunk.len() <= 1024 * 1024, "模型响应过大");
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).context("模型响应格式错误")?;
        let raw = if settings.api == "responses" {
            value["output"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|item| item["content"].as_array().into_iter().flatten())
                .filter_map(|v| v["text"].as_str())
                .collect::<String>()
        } else {
            value["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .into()
        };
        let raw = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        serde_json::from_str(raw).context("模型返回的识别结果格式错误")
    };
    tokio::select! {
        _ = app.stop.cancelled() => bail!("后台正在停止"),
        answer = tokio::time::timeout(Duration::from_secs(60), response) => answer.context("模型响应超时")?,
    }
}
