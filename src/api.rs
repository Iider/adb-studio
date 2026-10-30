use crate::{App, engine, schedule, types::*};
use anyhow::Context;
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Path, Query, Request, State,
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

struct Error(anyhow::Error, StatusCode);
impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(e: E) -> Self {
        Self(e.into(), StatusCode::BAD_REQUEST)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.1, Json(json!({"error":self.0.to_string()}))).into_response()
    }
}
type Result<T> = std::result::Result<T, Error>;

pub fn router(app: Arc<App>) -> Router {
    let secured = Router::new()
        .merge(crate::drafts::router())
        .merge(crate::workflow_transfer::router())
        .route("/devices", get(devices))
        .route("/devices/connect", post(connect_device))
        .route("/devices/discover", get(discover_devices))
        .route("/devices/pair", post(pair_device))
        .route("/devices/refresh", post(refresh))
        .route("/devices/restart-adb", post(restart_adb))
        .route("/devices/{id}", put(update_device))
        .route("/devices/{id}/screenshot", get(screenshot))
        .route("/devices/{id}/screen", post(screen_control))
        .route("/devices/{id}/mirror", get(mirror))
        .route("/workflows", get(workflows))
        .route("/workflows/{id}", axum::routing::delete(remove_workflow))
        .route("/workflow-drafts/{id}/save", post(save_draft))
        .route("/editor/observe/{id}", get(editor_observe))
        .route("/editor/captures/{id}", get(editor_capture))
        .route("/editor/test-step", post(test_step))
        .route("/workflows/{key}/validate", post(validate))
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
        .route(
            "/recognition/check",
            post(inspect_vision).layer(DefaultBodyLimit::max(12 * 1024 * 1024)),
        )
        .route("/recognition/samples/{name}", get(vision_sample))
        .route("/settings", get(settings))
        .route("/settings/model", put(set_model))
        .route("/shots/{run}/{file}", get(get_shot))
        .route("/ws", get(ws))
        .fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error":"接口不存在"}))) })
        .layer(middleware::from_fn_with_state(app.clone(), auth));
    Router::new()
        .route("/api/session", get(session))
        .nest("/api", secured)
        .nest_service("/_app", ServeDir::new(app.root.join("web/build/_app")))
        .fallback_service(
            ServeDir::new(app.root.join("web/build"))
                .fallback(ServeFile::new(app.root.join("web/build/index.html"))),
        )
        .layer(middleware::from_fn(web_cache))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(app)
}
async fn web_cache(request: Request, next: Next) -> Response {
    let immutable = request.uri().path().starts_with("/_app/immutable/");
    let mut response = next.run(request).await;
    if !immutable || response.status().as_u16() >= 400 {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    }
    response
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
    if !app.access.allows(h)
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
    if !app.access.allows(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    (
        [
            (
                header::SET_COOKIE,
                format!(
                    "adb_session={}; HttpOnly; SameSite=Strict; Path=/api{}",
                    app.token,
                    if app.access.secure(&headers) {
                        "; Secure"
                    } else {
                        ""
                    }
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
async fn discover_devices(
    State(app): State<Arc<App>>,
) -> Result<Json<Vec<crate::adb::NearbyDevice>>> {
    Ok(Json(crate::adb::discover(&app).await?))
}
async fn refresh(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    crate::adb::refresh(&app).await?;
    Ok(Json(json!({"ok":true})))
}
async fn restart_adb(State(app): State<Arc<App>>) -> Result<Json<Vec<Device>>> {
    Ok(Json(crate::adb::restart_server(&app).await?))
}
#[derive(Deserialize)]
struct DeviceConnect {
    address: String,
}
async fn connect_device(
    State(app): State<Arc<App>>,
    Json(input): Json<DeviceConnect>,
) -> Result<Json<Device>> {
    Ok(Json(
        crate::adb::connect_manual(&app, &input.address).await?,
    ))
}
#[derive(Deserialize)]
struct DevicePair {
    address: String,
    code: String,
}
async fn pair_device(
    State(app): State<Arc<App>>,
    Json(input): Json<DevicePair>,
) -> Result<Json<Value>> {
    crate::adb::pair_manual(&app, &input.address, &input.code).await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceUpdate {
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
        crate::mirror::ensure_screen_available(&app, &id).await?;
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
        let message = app.adb.screen_control(&id, wake).await?;
        let _gate = app.device_gate.lock().await;
        if let Some(mut device) = app.db.get::<Device>("devices", &id).await? {
            device.display_off = false;
            app.db.put("devices", &id, &device).await?;
        }
        message
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
async fn mirror(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    if app.db.get::<Device>("devices", &id).await?.is_none() {
        return Err(anyhow::anyhow!("手机未登记").into());
    }
    Ok(ws
        .on_upgrade(move |socket| async move {
            let worker = app.clone();
            let _ = app
                .workers
                .spawn(crate::mirror::serve(worker, id, socket))
                .await;
        })
        .into_response())
}
async fn workflows(State(app): State<Arc<App>>) -> Result<Json<Vec<Workflow>>> {
    Ok(Json(app.db.list("workflows").await?))
}

async fn remove_workflow(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let _schedule_gate = app.schedule_gate.lock().await;
    let _run_gate = app.run_gate.lock().await;
    if app.db.get::<Workflow>("workflows", &id).await?.is_none() {
        return Err(anyhow::anyhow!("任务不存在").into());
    }
    let runs = app.db.list::<Run>("runs").await?;
    let controls = app.controls.lock().await;
    if runs
        .iter()
        .any(|run| run.workflow.id == id && (run.busy() || controls.contains_key(&run.id)))
    {
        return Err(anyhow::anyhow!("任务正在运行或暂停，请先停止再删除").into());
    }
    drop(controls);
    let draft_id = id
        .strip_prefix("manual-")
        .filter(|value| uuid::Uuid::parse_str(value).is_ok())
        .unwrap_or("");
    app.db.remove_workflow(&id, draft_id).await?;
    let cleaned = match crate::workflow_cleanup::finish(&app, &id).await {
        Ok(cleaned) => cleaned,
        Err(error) => {
            tracing::warn!("任务 {id} 已删除，截图清理稍后重试：{error}");
            false
        }
    };
    let _ = app.events.send(json!({"type":"changed"}));
    Ok(Json(json!({"ok":true,"cleanup_pending":!cleaned})))
}

#[derive(Deserialize)]
struct DraftSave {
    revision: u32,
}
async fn save_draft(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<DraftSave>,
) -> Result<Json<Workflow>> {
    let _gate = app.run_gate.lock().await;
    let draft = app
        .db
        .get::<crate::drafts::Draft>("workflow_drafts", &id)
        .await?
        .context("草稿不存在")?;
    if draft.revision != input.revision {
        return Err(anyhow::anyhow!("草稿已变化，请先保存并重新读取").into());
    }
    let workflow = crate::canvas::compile(&draft)?;
    app.db.put("workflows", &workflow.id, &workflow).await?;
    let _ = app.events.send(json!({"type":"changed"}));
    Ok(Json(workflow))
}

#[derive(Deserialize, Default)]
struct ObserveOptions {
    #[serde(default)]
    retain: bool,
}

async fn editor_observe(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(options): Query<ObserveOptions>,
) -> Result<Json<Value>> {
    use base64::Engine;
    app.adb.check_unlocked(&id).await?;
    let screen = app.adb.canvas_screen(&id).await?;
    let png = app.adb.shot(&id).await?;
    let image = image::load_from_memory(&png)?;
    if (image.width(), image.height()) != (screen.width, screen.height) {
        return Err(anyhow::anyhow!("屏幕发生变化，请重新取图").into());
    }
    let capture_id = if options.retain {
        let id = uuid::Uuid::new_v4().to_string();
        let folder = app.data.join("vision-samples");
        tokio::fs::create_dir_all(&folder).await?;
        tokio::fs::write(folder.join(format!("{id}.png")), &png).await?;
        Some(id)
    } else {
        None
    };
    Ok(Json(
        json!({"screen":screen,"capture_id":capture_id,"captured_at":now(),"image":format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png))}),
    ))
}

async fn editor_capture(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Response> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(anyhow::anyhow!("截图编号无效").into());
    }
    let png = tokio::fs::read(app.data.join("vision-samples").join(format!("{id}.png")))
        .await
        .context("上次截图已不可用，请重新获取")?;
    Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestStep {
    draft: crate::drafts::Draft,
    node_id: String,
    device_id: String,
    #[serde(default)]
    inputs: crate::workflow_inputs::Values,
}

async fn test_step(State(app): State<Arc<App>>, Json(body): Json<TestStep>) -> Result<Json<Run>> {
    let workflow = crate::step_test::workflow(&body.draft, &body.node_id, &body.inputs)?;
    Ok(Json(
        engine::start_template(
            app,
            &body.device_id,
            workflow,
            body.inputs,
            Some(body.node_id),
        )
        .await?,
    ))
}
async fn validate(State(app): State<Arc<App>>, Path(key): Path<String>) -> Result<Json<Value>> {
    let w = app
        .db
        .get::<Workflow>("workflows", &key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("工作流不存在"))?;
    validate_workflow_samples(&app, &w).await?;
    let _gate = app.run_gate.lock().await;
    ensure_workflow(&app, &w).await?;
    Ok(Json(json!({"ok":true,"samples":w.tests.len()})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    #[serde(default)]
    inputs: crate::workflow_inputs::Values,
    device_id: String,
    workflow_id: String,
}
async fn start(State(app): State<Arc<App>>, Json(s): Json<Start>) -> Result<Json<Run>> {
    Ok(Json(
        engine::start_with_inputs(app, &s.device_id, &s.workflow_id, s.inputs).await?,
    ))
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
    let vision = app
        .db
        .list::<Value>("vision_results")
        .await?
        .into_iter()
        .filter(|item| item["run_id"] == id)
        .collect::<Vec<_>>();
    Ok(Json(json!({"run":r,"events":e,"vision":vision})))
}
async fn schedules(State(app): State<Arc<App>>) -> Result<Json<Vec<Schedule>>> {
    Ok(Json(app.db.list("schedules").await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleInput {
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
    engine::validate(&engine::workflow(app, &s.workflow_id).await?)?;
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
    let model_configured = crate::recognition::configured(&app, &s);
    Ok(Json(
        json!({"model":s,"model_configured":model_configured,"timezone":"Asia/Shanghai","retention_days":30,"host":format!("{}:{}",std::env::var("ADB_STUDIO_HOST").unwrap_or_else(|_|"127.0.0.1".into()),std::env::var("ADB_STUDIO_PORT").unwrap_or_else(|_|"17841".into())),"autostart":crate::platform::autostart(),"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"local_ocr":crate::platform::has_local_ocr(&app.root,&app.data)}),
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
    let _gate = app.run_gate.lock().await;
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

async fn ensure_workflow(app: &App, workflow: &Workflow) -> anyhow::Result<()> {
    anyhow::ensure!(
        app.db
            .get::<Workflow>("workflows", &workflow.id)
            .await?
            .is_some(),
        "任务已移除，请重新创建"
    );
    Ok(())
}

async fn validate_workflow_samples(_app: &App, workflow: &Workflow) -> anyhow::Result<()> {
    engine::samples(workflow)?;
    Ok(())
}
async fn inspect_vision(
    State(app): State<Arc<App>>,
    Json(request): Json<crate::vision::Inspect>,
) -> Result<Json<crate::vision::Inspection>> {
    Ok(Json(crate::vision::inspect(&app, request).await?))
}
async fn vision_sample(State(app): State<Arc<App>>, Path(name): Path<String>) -> Result<Response> {
    let frame = crate::vision::sample_frame(&app, &name).await?;
    Ok(([(header::CONTENT_TYPE, "image/png")], frame.png).into_response())
}
