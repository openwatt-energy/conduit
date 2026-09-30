use crate::auth::CompositeAuth;
use crate::config::RegistryConfig;
use crate::mqtt::protocol::Packet;
use crate::mqtt::server::ServerStats;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Json, Response};
use axum::routing::{get, post};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct AppState {
    pub stats: ServerStats,
    pub influx_flushed: Arc<std::sync::atomic::AtomicUsize>,
    pub influx_failed: Arc<std::sync::atomic::AtomicUsize>,
    pub auth: Arc<CompositeAuth>,
    pub command_subs: Arc<
        tokio::sync::RwLock<HashMap<String, mpsc::UnboundedSender<Packet>>>,
    >,
    pub registry_config: RegistryConfig,
    pub admin_token: String,
    pub uptime: std::time::Instant,
    pub tls_enabled: bool,
    pub plain_enabled: bool,
}

pub fn router(state: AppState) -> axum::Router {
    let authed = axum::Router::new()
        .route("/status", get(status))
        .route("/metrics", get(metrics))
        .route("/admin/reload", post(admin_reload))
        .route("/admin/clients", get(admin_clients))
        .route("/command/{device_id}", post(command_send))
        .layer(middleware::from_fn_with_state(
            state.admin_token.clone(),
            require_auth,
        ));

    axum::Router::new()
        .route("/health", get(health))
        .merge(authed)
        .with_state(state)
}

async fn require_auth(
    State(token): State<String>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let authed = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v == token)
        .unwrap_or(false);

    if !authed {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(Serialize)]
struct StatusResponse {
    version: &'static str,
    uptime_secs: u64,
    tls_listener: bool,
    plain_listener: bool,
    connected_devices: usize,
    messages_processed: usize,
    auth_failures: usize,
    parse_errors: usize,
    influx_flushed: usize,
    influx_failed: usize,
    registry_entries: usize,
    recent_auth_failures: Vec<crate::mqtt::server::AuthFailure>,
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION"),
        uptime_secs: state.uptime.elapsed().as_secs(),
        tls_listener: state.tls_enabled,
        plain_listener: state.plain_enabled,
        connected_devices: state
            .stats
            .connections
            .load(std::sync::atomic::Ordering::Relaxed),
        messages_processed: state
            .stats
            .messages_processed
            .load(std::sync::atomic::Ordering::Relaxed),
        auth_failures: state
            .stats
            .auth_failures
            .load(std::sync::atomic::Ordering::Relaxed),
        parse_errors: state
            .stats
            .parse_errors
            .load(std::sync::atomic::Ordering::Relaxed),
        influx_flushed: state
            .influx_flushed
            .load(std::sync::atomic::Ordering::Relaxed),
        influx_failed: state
            .influx_failed
            .load(std::sync::atomic::Ordering::Relaxed),
        registry_entries: state.auth.registry().len(),
        recent_auth_failures: state
            .stats
            .recent_auth_failures
            .lock()
            .unwrap()
            .clone(),
    })
}

#[derive(Serialize)]
struct PromMetric {
    name: String,
    value: f64,
    labels: HashMap<String, String>,
    help: String,
    metric_type: String,
}

async fn metrics(State(state): State<AppState>) -> (StatusCode, String) {
    let uptime = state.uptime.elapsed().as_secs_f64();
    let msgs = state
        .stats
        .messages_processed
        .load(std::sync::atomic::Ordering::Relaxed) as f64;
    let rate = if uptime > 0.0 { msgs / uptime } else { 0.0 };

    let mut out = String::new();

    let metrics = vec![
        PromMetric {
            name: "owmqtt_connections".into(),
            value: state.stats.connections.load(std::sync::atomic::Ordering::Relaxed) as f64,
            labels: HashMap::new(),
            help: "Active MQTT connections".into(),
            metric_type: "gauge".into(),
        },
        PromMetric {
            name: "owmqtt_messages_total".into(),
            value: msgs,
            labels: HashMap::new(),
            help: "Total MQTT messages processed".into(),
            metric_type: "counter".into(),
        },
        PromMetric {
            name: "owmqtt_messages_rate".into(),
            value: rate,
            labels: HashMap::new(),
            help: "Messages per second".into(),
            metric_type: "gauge".into(),
        },
        PromMetric {
            name: "owmqtt_auth_failures_total".into(),
            value: state.stats.auth_failures.load(std::sync::atomic::Ordering::Relaxed) as f64,
            labels: HashMap::new(),
            help: "Total authentication failures".into(),
            metric_type: "counter".into(),
        },
        PromMetric {
            name: "owmqtt_parse_errors_total".into(),
            value: state.stats.parse_errors.load(std::sync::atomic::Ordering::Relaxed) as f64,
            labels: HashMap::new(),
            help: "Total telemetry parse errors".into(),
            metric_type: "counter".into(),
        },
        PromMetric {
            name: "owmqtt_influx_flushed_total".into(),
            value: state.influx_flushed.load(std::sync::atomic::Ordering::Relaxed) as f64,
            labels: HashMap::new(),
            help: "Total points written to InfluxDB".into(),
            metric_type: "counter".into(),
        },
        PromMetric {
            name: "owmqtt_influx_failed_total".into(),
            value: state.influx_failed.load(std::sync::atomic::Ordering::Relaxed) as f64,
            labels: HashMap::new(),
            help: "Total points failed to write to InfluxDB".into(),
            metric_type: "counter".into(),
        },
    ];

    for m in &metrics {
        out.push_str(&format!("# HELP {} {}\n", m.name, m.help));
        out.push_str(&format!("# TYPE {} {}\n", m.name, m.metric_type));
        if m.labels.is_empty() {
            out.push_str(&format!("{} {}\n", m.name, m.value));
        } else {
            let labels: Vec<String> = m
                .labels
                .iter()
                .map(|(k, v)| format!("{k}=\"{v}\""))
                .collect();
            out.push_str(&format!("{}{{{}}} {}\n", m.name, labels.join(","), m.value));
        }
    }

    (StatusCode::OK, out)
}

async fn admin_reload(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = reqwest::Client::new();
    let resp = client
        .get(&state.registry_config.url)
        .header(
            "Authorization",
            format!("Bearer {}", state.registry_config.api_key),
        )
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    if !resp.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let json: serde_json::Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    let entries: Vec<crate::auth::registry::DeviceEntry> = match &json {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| {
                let dongle = v.get("dongle")?.as_str()?.to_string();
                let auth = v.get("auth")?.as_str()?.to_string();
                Some(crate::auth::registry::DeviceEntry { dongle, auth })
            })
            .collect(),
        _ => {
            if let Some(devices) = json.get("devices").and_then(|v| v.as_array()) {
                devices
                    .iter()
                    .filter_map(|v| {
                        let dongle = v.get("dongle")?.as_str()?.to_string();
                        let auth = v.get("auth")?.as_str()?.to_string();
                        Some(crate::auth::registry::DeviceEntry { dongle, auth })
                    })
                    .collect()
            } else {
                return Err(StatusCode::BAD_GATEWAY);
            }
        }
    };

    let count = entries.len();
    state.auth.update_registry(entries);

    Ok(Json(serde_json::json!({
        "status": "ok",
        "entries_loaded": count,
    })))
}

async fn admin_clients(State(state): State<AppState>) -> Json<serde_json::Value> {
    let entries = state.auth.registry();
    Json(serde_json::json!({
        "count": entries.len(),
        "clients": entries,
    }))
}

#[derive(serde::Deserialize)]
struct CommandRequest {
    payload: serde_json::Value,
}

async fn command_send(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    axum::Json(body): axum::Json<CommandRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let payload_str = serde_json::to_string(&body.payload).unwrap_or_default();

    let subs = state.command_subs.read().await;
    let tx = subs.get(&device_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "device not found"})),
        )
    })?;

    let publish = Packet::Publish(crate::mqtt::protocol::Publish {
        dup: false,
        qos: 1,
        retain: false,
        topic: format_command_topic(&device_id),
        packet_id: None,
        payload: bytes::Bytes::from(payload_str),
    });

    tx.send(publish).map_err(|_| {
        (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "device not connected"})),
        )
    })?;

    Ok(Json(serde_json::json!({
        "status": "delivered",
        "device_id": device_id,
    })))
}

fn format_command_topic(device_id: &str) -> String {
    if device_id.starts_with("P1M5_") {
        format!("P1M5/{}/command", &device_id[5..])
    } else if device_id.starts_with("ow_") {
        format!("BEPR/{}/command", &device_id[3..])
    } else {
        format!("{device_id}/command")
    }
}