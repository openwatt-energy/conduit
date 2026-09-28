use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Default)]
struct MockState {
    write_count: Arc<std::sync::atomic::AtomicUsize>,
    fail_mode: Arc<std::sync::atomic::AtomicBool>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::var("CONDUIT_MOCK_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8086".into());

    let state = MockState::default();

    let app = axum::Router::new()
        .route("/health", get(health))
        .route("/api/v2/write", post(write))
        .route("/mock/fail", post(set_fail))
        .route("/mock/recover", post(clear_fail))
        .route("/mock/stats", get(stats))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("InfluxDB mock listening on {addr}");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

async fn write(
    State(state): State<MockState>,
    body: String,
) -> Result<(StatusCode, &'static str), StatusCode> {
    if state.fail_mode.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    let count = lines.len();
    state
        .write_count
        .fetch_add(count, std::sync::atomic::Ordering::Relaxed);

    for line in &lines {
        if !line.contains(',') || !line.contains(' ') {
            return Err(StatusCode::BAD_REQUEST);
        }
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() < 2 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    Ok((StatusCode::NO_CONTENT, ""))
}

async fn set_fail(State(state): State<MockState>) -> &'static str {
    state
        .fail_mode
        .store(true, std::sync::atomic::Ordering::Relaxed);
    "fail mode ON"
}

async fn clear_fail(State(state): State<MockState>) -> &'static str {
    state
        .fail_mode
        .store(false, std::sync::atomic::Ordering::Relaxed);
    "fail mode OFF"
}

#[derive(Serialize)]
struct StatsResponse {
    writes_received: usize,
    fail_mode: bool,
}

async fn stats(State(state): State<MockState>) -> Json<StatsResponse> {
    Json(StatsResponse {
        writes_received: state
            .write_count
            .load(std::sync::atomic::Ordering::Relaxed),
        fail_mode: state.fail_mode.load(std::sync::atomic::Ordering::Relaxed),
    })
}