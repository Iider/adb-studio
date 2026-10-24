use serde::{Deserialize, Serialize};
use serde_json::Value;
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Device {
    pub id: String,
    #[serde(default)]
    pub tap_point: Option<TapPoint>,
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
            tap_point: None,
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
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub name: String,
    pub description: String,
    pub devices: Vec<String>,
    pub entry: String,
    pub nodes: Vec<Node>,
    #[serde(default = "ten")]
    pub model_limit: u32,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub sample_validated: bool,
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
fn ten() -> u32 {
    10
}
impl Workflow {
    pub fn key(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }
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
    VisionCheck {
        package: String,
        condition: String,
    },
    VisionTap {
        package: String,
        guard: String,
        target: String,
        region: [f64; 4],
        verify: String,
    },
    Wait {
        seconds: f64,
    },
    Launch {
        package: String,
    },
    TapText {
        text: String,
        verify: String,
    },
    TapPoint {
        x: f64,
        y: f64,
        guard: String,
        verify: String,
    },
    Swipe {
        from: [f64; 2],
        to: [f64; 2],
        duration_ms: u32,
    },
    Key {
        key: String,
        verify: String,
    },
    Branch {
        cases: Vec<Branch>,
        otherwise: String,
        #[serde(default)]
        guard: Option<String>,
    },
    FixedTapLoop {
        x: u32,
        y: u32,
        count: u32,
        interval: f64,
    },
    TapLoop {
        text: String,
        guard: String,
        package: String,
        region: [f64; 4],
        count: u32,
        interval: f64,
    },
    SwipeLoop {
        interval: f64,
        count: Option<u32>,
        minutes: Option<f64>,
        #[serde(default)]
        long: bool,
    },
    QishuiNavigate,
    QishuiLoop {
        coin_region: [f64; 4],
        identity_region: [f64; 4],
        warmup: f64,
        stall: f64,
        finish_stall: f64,
        finish_swipes: u32,
    },
    End {
        result: String,
    },
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Branch {
    pub contains: String,
    pub next: String,
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
    pub id: String,
    pub device_id: String,
    pub workflow: Workflow,
    pub state: String,
    pub node: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub reason: String,
    pub model_calls: u32,
    pub trial: bool,
    pub checkpoint: Value,
    pub pending: Option<String>,
    pub last_shot: Option<String>,
    #[serde(default)]
    pub error_detail: Option<String>,
    #[serde(default)]
    pub resume_app: Option<String>,
    #[serde(default)]
    pub resume_size: Option<[u32; 2]>,
    #[serde(default)]
    pub model_settings: Option<ModelSettings>,
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
pub struct TapPoint {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl TapPoint {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.width == 0
            || self.height == 0
            || self.width > 10000
            || self.height > 10000
            || self.x >= self.width
            || self.y >= self.height
        {
            anyhow::bail!("点击位置超出截图范围")
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TapSettings {
    pub count: u32,
    pub interval: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SwipeSettings {
    pub minutes: f64,
    pub interval: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub device_id: String,
    pub workflow_id: String,
    #[serde(default)]
    pub swipe: Option<SwipeSettings>,
    #[serde(default)]
    pub tap: Option<TapSettings>,
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
