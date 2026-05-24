use anyhow::{Context, Result};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};

#[derive(Serialize)]
struct EngineRequest {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
}

#[derive(Deserialize, Debug)]
struct EngineResponse {
    ok: bool,
    data: Option<serde_json::Value>,
    error: Option<String>,
}

#[derive(Clone)]
struct AppState {
    socket_path: PathBuf,
}

fn get_socket_path() -> Result<PathBuf> {
    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .context("XDG_RUNTIME_DIR environment variable not set")?;
    Ok(PathBuf::from(runtime_dir).join("track-me.sock"))
}

/// Send a synchronous request to the track-me background engine
fn query_engine(socket_path: &PathBuf, req: EngineRequest) -> Result<serde_json::Value> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| "Failed to connect to the track-me engine. Is the service running?")?;

    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let mut req_str = serde_json::to_string(&req)?;
    req_str.push('\n');

    stream.write_all(req_str.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut response_str = String::new();
    reader.read_line(&mut response_str)?;

    let response: EngineResponse = serde_json::from_str(&response_str)
        .context("Failed to parse response from engine")?;

    if response.ok {
        Ok(response.data.unwrap_or(serde_json::Value::Null))
    } else {
        anyhow::bail!(
            "Engine error: {}",
            response.error.unwrap_or_else(|| "Unknown error".into())
        )
    }
}

// --- Handlers ---

async fn handle_status(State(state): State<AppState>) -> impl IntoResponse {
    let req = EngineRequest {
        command: "current".into(),
        date: None,
        from: None,
        to: None,
    };
    match query_engine(&state.socket_path, req) {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_today(State(state): State<AppState>) -> impl IntoResponse {
    let req = EngineRequest {
        command: "today".into(),
        date: None,
        from: None,
        to: None,
    };
    match query_engine(&state.socket_path, req) {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_date(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> impl IntoResponse {
    let req = EngineRequest {
        command: "date".into(),
        date: Some(date),
        from: None,
        to: None,
    };
    match query_engine(&state.socket_path, req) {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_range(
    State(state): State<AppState>,
    Path((from, to)): Path<(String, String)>,
) -> impl IntoResponse {
    let req = EngineRequest {
        command: "range".into(),
        date: None,
        from: Some(from),
        to: Some(to),
    };
    match query_engine(&state.socket_path, req) {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let socket_path = get_socket_path()?;
    log::info!("API will query engine socket at: {:?}", socket_path);

    let state = AppState { socket_path };

    // CORS layer to allow frontend (e.g. Vite on localhost:5173) to fetch data
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/status", get(handle_status))
        .route("/api/today", get(handle_today))
        .route("/api/date/:date", get(handle_date))
        .route("/api/range/:from/:to", get(handle_range))
        .layer(cors)
        .with_state(state);

    let addr = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("API Server running at http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
