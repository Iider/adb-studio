use crate::{App, engine, schedule, types::*};
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Path, Request, State,
        ws::{Message, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

struct Error(anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":self.0.to_string()})),
        )
            .into_response()
    }
}
type Result<T> = std::result::Result<T, Error>;

pub fn router(app: Arc<App>) -> Router {
    let secured = Router::new()
        .route("/devices", get(devices))
        .route("/devices/refresh", post(refresh))
        .route("/devices/{id}", put(update_device))
        .route("/devices/{id}/screenshot", get(screenshot))
        .route("/devices/{id}/screen", post(screen_control))
        .route("/workflows", get(workflows).post(submit))
        .route("/workflows/{key}/validate", post(validate))
        .route("/workflows/{key}/publish", post(publish))
        .route("/runs", get(runs).post(start))
        .route("/runs/{id}", axum::routing::delete(clear_run))
        .route("/runs/{id}/stop", post(stop))
        .route("/runs/{id}/pause", post(pause_run))
        .route("/runs/{id}/resume", post(resume_run))
        .route("/runs/{id}/export", get(export))
        .route("/runs/{id}/events", get(events))
        .route("/schedules", get(schedules).post(add_schedule))
        .route(
            "/schedules/{id}",
            put(update_schedule).delete(remove_schedule),
        )
        .route("/settings", get(settings))
        .route("/settings/model", put(set_model))
        .route("/shots/{run}/{file}", get(get_shot))
        .route("/ws", get(ws))
        .fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error":"接口不存在"}))) })
        .layer(middleware::from_fn_with_state(app.clone(), auth));
    Router::new()
        .route("/api/session", get(session))
        .nest("/api", secured)
        .fallback_service(
            ServeDir::new(app.root.join("web/build"))
                .not_found_service(ServeFile::new(app.root.join("web/build/index.html"))),
        )
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(app)
}
fn allowed_origin(headers: &HeaderMap) -> bool {
    let host = headers
        .get(header::HOST)
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");
    let Some((name, port)) = host.rsplit_once(':') else {
        return false;
    };
    let lan = std::env::var("ADB_STUDIO_HOST").as_deref() == Ok("0.0.0.0");
    let allowed_host = ["127.0.0.1", "localhost"].contains(&name)
        || (lan && name.parse::<std::net::Ipv4Addr>().is_ok());
    if !allowed_host || port.parse::<u16>().is_err() {
        return false;
    }
    if headers
        .get("sec-fetch-site")
        .is_some_and(|s| s == "cross-site")
    {
        return false;
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        if origin != format!("http://{host}").as_str() {
            return false;
        }
    }
    true
}
async fn auth(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    let h = request.headers();
    let authorization = h.get(header::AUTHORIZATION).and_then(|s| s.to_str().ok());
    let cookie = h
        .get(header::COOKIE)
        .and_then(|s| s.to_str().ok())
        .is_some_and(|s| {
            s.split(';')
                .any(|p| p.trim() == format!("adb_session={}", app.token))
        });
    if !allowed_origin(h)
        || !(authorization == Some(format!("Bearer {}", app.token).as_str()) || cookie)
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"访问令牌无效"})),
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
async fn session(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if !allowed_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    (
        [
            (
                header::SET_COOKIE,
                format!(
                    "adb_session={}; HttpOnly; SameSite=Strict; Path=/api",
                    app.token
                ),
            ),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
        Json(json!({"token":app.token})),
    )
        .into_response()
}
async fn devices(State(app): State<Arc<App>>) -> Result<Json<Vec<Device>>> {
    Ok(Json(app.db.list("devices").await?))
}
async fn refresh(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    crate::adb::refresh(&app).await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct DeviceUpdate {
    #[serde(default)]
    tap_point: Option<TapPoint>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    keep_awake: Option<bool>,
}
async fn update_device(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(u): Json<DeviceUpdate>,
) -> Result<Json<Device>> {
    let _gate = app.device_gate.lock().await;
    let mut d = app
        .db
        .get::<Device>("devices", &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("手机未登记"))?;
    if let Some(point) = u.tap_point {
        point.validate()?;
        d.tap_point = Some(point);
    }
    if let Some(name) = u.name {
        if name.trim().is_empty() || name.chars().count() > 40 {
            return Err(anyhow::anyhow!("名称需为 1 至 40 字").into());
        }
        d.name = name.trim().into();
    }
    if let Some(keep) = u.keep_awake {
        d.keep_awake = keep;
        d.restore_pending = !keep && d.original_awake.is_some();
        app.db.put("devices", &id, &d).await?;
        if d.online {
            if let Err(e) = crate::adb::apply_awake(&app, &mut d).await {
                d.note = format!("设置待处理：{e}")
            }
        } else {
            d.note = if keep {
                "上线后开启常亮"
            } else {
                "上线后恢复原设置"
            }
            .into();
        }
    }
    app.db.put("devices", &id, &d).await?;
    let _ = app.events.send(json!({"type":"changed"}));
    Ok(Json(d))
}
#[derive(Deserialize)]
struct ScreenInput {
    action: String,
}
async fn screen_control(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<ScreenInput>,
) -> Result<Json<Value>> {
    let wake = match input.action.as_str() {
        "wake" => true,
        "lock" => false,
        _ => return Err(anyhow::anyhow!("屏幕操作仅支持 wake 或 lock").into()),
    };
    let message = {
        let _gate = app.run_gate.lock().await;
        if app.db.get::<Device>("devices", &id).await?.is_none() {
            return Err(anyhow::anyhow!("手机未登记").into());
        }
        if app
            .db
            .list::<Run>("runs")
            .await?
            .iter()
            .any(|run| run.device_id == id && run.busy() && run.state != "paused")
        {
            return Err(anyhow::anyhow!("请先暂停接管或停止这台手机的任务，再操作屏幕").into());
        }
        app.adb.screen_control(&id, wake).await?
    };
    crate::adb::refresh(&app).await?;
    Ok(Json(json!({"message":message})))
}

async fn screenshot(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Response> {
    let shot = app.adb.capture(&id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png".to_owned()),
            (
                header::HeaderName::from_static("server-timing"),
                shot.timing.header(),
            ),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        shot.png,
    )
        .into_response())
}
async fn workflows(State(app): State<Arc<App>>) -> Result<Json<Vec<Workflow>>> {
    let mut rows = app.db.list::<Workflow>("workflows").await?;
    for w in &mut rows {
        w.active = app
            .db
            .get::<String>("active", &w.id)
            .await?
            .is_some_and(|key| key == w.key());
    }
    Ok(Json(rows))
}
async fn submit(
    State(app): State<Arc<App>>,
    Json(mut w): Json<Workflow>,
) -> Result<Json<Workflow>> {
    let _gate = app.run_gate.lock().await;
    engine::validate(&w)?;
    if app
        .db
        .get::<Workflow>("workflows", &w.key())
        .await?
        .is_some()
    {
        return Err(anyhow::anyhow!("版本已存在，请增加版本号").into());
    }
    w.active = false;
    w.sample_validated = false;
    app.db.put("workflows", &w.key(), &w).await?;
    Ok(Json(w))
}
async fn validate(State(app): State<Arc<App>>, Path(key): Path<String>) -> Result<Json<Value>> {
    let mut w = app
        .db
        .get::<Workflow>("workflows", &key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("工作流不存在"))?;
    engine::samples(&w)?;
    w.sample_validated = true;
    app.db.put("workflows", &key, &w).await?;
    Ok(Json(json!({"ok":true,"samples":w.tests.len()})))
}
async fn publish(State(app): State<Arc<App>>, Path(key): Path<String>) -> Result<Json<Value>> {
    let _gate = app.run_gate.lock().await;
    let w = app
        .db
        .get::<Workflow>("workflows", &key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("工作流不存在"))?;
    engine::activate(&app, &w, false).await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct Start {
    #[serde(default)]
    swipe: Option<SwipeSettings>,
    #[serde(default)]
    tap: Option<TapSettings>,
    device_id: String,
    workflow_id: String,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    trial: bool,
}
async fn start(State(app): State<Arc<App>>, Json(s): Json<Start>) -> Result<Json<Run>> {
    let w = if s.trial {
        let version = s.version.ok_or_else(|| anyhow::anyhow!("试跑需指定版本"))?;
        let key = format!("{}@{version}", s.workflow_id);
        let mut w = app
            .db
            .get::<Workflow>("workflows", &key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("版本不存在"))?;
        engine::samples(&w)?;
        w.sample_validated = true;
        app.db.put("workflows", &key, &w).await?;
        w
    } else {
        engine::active(&app, &s.workflow_id).await?
    };
    let w = engine::with_swipe_settings(w, s.swipe.as_ref())?;
    let w = engine::with_tap_settings(w, s.tap.as_ref())?;
    Ok(Json(engine::start(app, &s.device_id, w, s.trial).await?))
}
async fn runs(State(app): State<Arc<App>>) -> Result<Json<Vec<Value>>> {
    let rows = app.db.list::<Run>("runs").await?;
    let controls = app.controls.lock().await;
    Ok(Json(
        rows.into_iter()
            .map(|run| {
                let requested = controls
                    .get(&run.id)
                    .is_some_and(|control| control.pause.is_cancelled());
                let mut value = json!(run);
                value["pause_requested"] = json!(requested);
                value
            })
            .collect(),
    ))
}
async fn clear_run(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let _gate = app.run_gate.lock().await;
    let run = app
        .db
        .get::<Run>("runs", &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("记录不存在"))?;
    if run.busy() {
        return Err(anyhow::anyhow!("请先停止正在执行的任务").into());
    }
    // 保留归档和截图，清理后仍可恢复原始执行证据。
    app.db.put("archived_runs", &id, &run).await?;
    for event in app
        .db
        .list::<Value>("events")
        .await?
        .into_iter()
        .filter(|event| event["run_id"] == id)
    {
        if let Some(key) = event["id"].as_str() {
            app.db.put("archived_events", key, &event).await?;
            app.db.delete("events", key).await?;
        }
    }
    app.db.delete("runs", &id).await?;
    let _ = app.events.send(json!({"type":"changed"}));
    Ok(Json(json!({"cleared": id})))
}

async fn stop(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let _gate = app.run_gate.lock().await;
    let control = app.controls.lock().await.get(&id).cloned();
    if let Some(control) = control {
        control.stop.cancel();
    } else {
        let mut run = app
            .db
            .get::<Run>("runs", &id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
        if run.state != "paused" {
            return Err(anyhow::anyhow!("任务未运行").into());
        }
        run.state = "cancelled".into();
        run.reason = "用户停止".into();
        run.ended_at = Some(now());
        app.db.put("runs", &id, &run).await?;
        app.event(&run, &run.reason).await?;
    }
    Ok(Json(json!({"ok":true})))
}
async fn pause_run(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    engine::request_pause(&app, &id).await?;
    Ok(Json(json!({"ok":true})))
}
async fn resume_run(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Run>> {
    Ok(Json(engine::resume(app, &id).await?))
}
async fn events(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Vec<Value>>> {
    Ok(Json(
        app.db
            .list::<Value>("events")
            .await?
            .into_iter()
            .filter(|e| e["run_id"] == id)
            .collect(),
    ))
}
async fn export(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let r = app
        .db
        .get::<Run>("runs", &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
    let e = app
        .db
        .list::<Value>("events")
        .await?
        .into_iter()
        .filter(|e| e["run_id"] == id)
        .collect::<Vec<_>>();
    Ok(Json(json!({"run":r,"events":e})))
}
async fn schedules(State(app): State<Arc<App>>) -> Result<Json<Vec<Schedule>>> {
    Ok(Json(app.db.list("schedules").await?))
}
#[derive(Deserialize)]
struct ScheduleInput {
    #[serde(default)]
    swipe: Option<SwipeSettings>,
    #[serde(default)]
    tap: Option<TapSettings>,
    name: String,
    device_id: String,
    workflow_id: String,
    rule: ScheduleRule,
    #[serde(default = "enabled")]
    enabled: bool,
}
fn enabled() -> bool {
    true
}
async fn schedule_from(app: &App, s: ScheduleInput, id: String) -> Result<Schedule> {
    if app
        .db
        .get::<Device>("devices", &s.device_id)
        .await?
        .is_none()
    {
        return Err(anyhow::anyhow!("手机未登记").into());
    }
    engine::with_tap_settings(
        engine::with_swipe_settings(engine::active(app, &s.workflow_id).await?, s.swipe.as_ref())?,
        s.tap.as_ref(),
    )?;
    if s.name.trim().is_empty() || s.name.chars().count() > 80 {
        return Err(anyhow::anyhow!("请输入定时名称，最多 80 字").into());
    }
    let now = chrono::Utc::now();
    let next = schedule::next(&s.rule, now, now)?.ok_or_else(|| anyhow::anyhow!("时间已过去"))?;
    Ok(Schedule {
        id,
        name: s.name.trim().into(),
        device_id: s.device_id,
        workflow_id: s.workflow_id,
        swipe: s.swipe,
        tap: s.tap,
        rule: s.rule,
        enabled: s.enabled,
        next_at: next.to_rfc3339(),
        created_at: now.to_rfc3339(),
        last_result: None,
    })
}
async fn add_schedule(
    State(app): State<Arc<App>>,
    Json(s): Json<ScheduleInput>,
) -> Result<Json<Schedule>> {
    let _gate = app.schedule_gate.lock().await;
    let s = schedule_from(&app, s, uuid::Uuid::new_v4().to_string()).await?;
    app.db.put("schedules", &s.id, &s).await?;
    Ok(Json(s))
}
async fn update_schedule(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(s): Json<ScheduleInput>,
) -> Result<Json<Schedule>> {
    let _gate = app.schedule_gate.lock().await;
    if app.db.get::<Schedule>("schedules", &id).await?.is_none() {
        return Err(anyhow::anyhow!("定时不存在").into());
    }
    let s = schedule_from(&app, s, id).await?;
    app.db.put("schedules", &s.id, &s).await?;
    Ok(Json(s))
}
async fn remove_schedule(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let _gate = app.schedule_gate.lock().await;
    app.db.delete("schedules", &id).await?;
    Ok(Json(json!({"ok":true})))
}
async fn settings(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let s = app
        .db
        .get::<ModelSettings>("settings", "model")
        .await?
        .unwrap_or_default();
    Ok(Json(
        json!({"model":s,"timezone":"Asia/Shanghai","retention_days":30,"host":format!("{}:{}",std::env::var("ADB_STUDIO_HOST").unwrap_or_else(|_|"127.0.0.1".into()),std::env::var("ADB_STUDIO_PORT").unwrap_or_else(|_|"17841".into())),"autostart":std::env::var("HOME").ok().is_some_and(|p|std::path::Path::new(&p).join("Library/LaunchAgents/local.adb-studio.plist").exists())}),
    ))
}
async fn set_model(
    State(app): State<Arc<App>>,
    Json(s): Json<ModelSettings>,
) -> Result<Json<Value>> {
    let url = reqwest::Url::parse(&s.endpoint)?;
    if !(url.scheme() == "https"
        || (url.scheme() == "http"
            && [Some("localhost"), Some("127.0.0.1")].contains(&url.host_str())))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(anyhow::anyhow!("接口需为 HTTPS 或本机地址").into());
    }
    if s.model.trim().is_empty()
        || !["responses", "chat_completions"].contains(&s.api.as_str())
        || s.key_env.is_empty()
        || !s
            .key_env
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(anyhow::anyhow!("模型或密钥变量无效").into());
    }
    app.db.put("settings", "model", &s).await?;
    Ok(Json(json!({"ok":true})))
}
async fn get_shot(
    State(app): State<Arc<App>>,
    Path((run, file)): Path<(String, String)>,
) -> Result<Response> {
    if uuid::Uuid::parse_str(&run).is_err()
        || !file.ends_with(".png")
        || !file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        || file.contains("..")
    {
        return Err(anyhow::anyhow!("截图路径无效").into());
    }
    let png = tokio::fs::read(app.data.join("shots").join(run).join(file)).await?;
    Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response())
}
async fn ws(State(app): State<Arc<App>>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move|mut socket|async move{let mut rx=app.events.subscribe();loop{tokio::select!{event=rx.recv()=>{match event{Ok(v)=>if socket.send(Message::Text(v.to_string().into())).await.is_err(){break},Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{if socket.send(Message::Text(json!({"type":"changed"}).to_string().into())).await.is_err(){break}},Err(_)=>break}},msg=socket.recv()=>if !matches!(msg,Some(Ok(Message::Ping(_)|Message::Pong(_)|Message::Text(_)))){break},_=app.stop.cancelled()=>break}}}).into_response()
}
