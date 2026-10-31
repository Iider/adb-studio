use serde::{Deserialize, Serialize};
use serde_json::Value;
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub model: String,
    pub online: bool,
    pub endpoints: Vec<String>,
    #[serde(default)]
    pub manual_endpoints: Vec<String>,
    pub last_seen: Option<String>,
    pub connection: String,
    pub keep_awake: bool,
    #[serde(default)]
    pub display_off: bool,
    pub original_awake: Option<String>,
    pub original_timeout: Option<String>,
    pub awake_applied: bool,
    pub restore_pending: bool,
    pub locked: bool,
    pub note: String,
}
impl Device {
    pub fn new(id: &str, name: &str, model: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            model: model.into(),
            online: false,
            endpoints: vec![],
            manual_endpoints: vec![],
            last_seen: None,
            connection: "未连接".into(),
            keep_awake: false,
            display_off: false,
            original_awake: None,
            original_timeout: None,
            awake_applied: false,
            restore_pending: false,
            locked: false,
            note: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Workflow {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<crate::workflow_inputs::Input>,
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub devices: Vec<String>,
    pub entry: String,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub tests: Vec<SampleCase>,
    #[serde(default)]
    pub recognition: RecognitionMode,
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionMode {
    #[default]
    Local,
    Vision,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    #[serde(flatten)]
    pub action: Action,
    #[serde(default)]
    pub next: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Canvas { step: crate::drafts::DraftNode },
    End { result: String },
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SampleCase {
    pub node: String,
    #[serde(default)]
    pub text: String,
    pub expected: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub after: bool,
    #[serde(default)]
    pub target_region: Option<[f64; 4]>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Run {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub random_samples: crate::random_values::Samples,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub random_fields: crate::random_values::Fields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub single_step: Option<String>,
    #[serde(default)]
    pub inputs: crate::workflow_inputs::Values,
    #[serde(default)]
    pub input_overrides: crate::workflow_inputs::Values,
    #[serde(default = "empty_object")]
    pub canvas_progress: Value,
    pub id: String,
    pub device_id: String,
    pub workflow: Workflow,
    pub state: String,
    pub node: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub reason: String,
    pub model_calls: u32,
    pub checkpoint: Value,
    pub pending: Option<String>,
    pub last_shot: Option<String>,
    #[serde(default)]
    pub error_detail: Option<String>,
    #[serde(default)]
    pub resume_size: Option<[u32; 2]>,
    #[serde(default)]
    pub model_settings: Option<ModelSettings>,
    #[serde(default)]
    pub text_recognition: Option<TextRecognition>,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TextRecognition {
    Local,
    Model,
}

fn empty_object() -> Value {
    serde_json::json!({})
}
impl Run {
    pub fn busy(&self) -> bool {
        matches!(
            self.state.as_str(),
            "running" | "waiting_device" | "recovering" | "paused"
        )
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub device_id: String,
    pub workflow_id: String,
    pub enabled: bool,
    pub rule: ScheduleRule,
    pub next_at: String,
    pub last_result: Option<String>,
    pub created_at: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScheduleRule {
    Once { at: String },
    Daily { time: String },
    Weekly { time: String, days: Vec<u32> },
    Interval { minutes: u32 },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct ModelSettings {
    pub endpoint: String,
    pub model: String,
    pub api: String,
    pub key_env: String,
}
impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            endpoint: "https://ark.cn-beijing.volces.com/api/v3/responses".into(),
            model: "doubao-seed-2-1-lite-260915".into(),
            api: "responses".into(),
            key_env: "MODEL_KEY".into(),
        }
    }
}
