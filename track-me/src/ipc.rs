// track-me: IPC server
//
// Exposes a Unix socket that accepts JSON queries and returns
// JSON responses. This is how external tools (CLI, TUI, web UI)
// communicate with the running engine.
//
// Protocol:
//   Client sends a single JSON line:  {"command": "today"}
//   Server responds with a JSON line: {"ok": true, "data": {...}}
//   Connection is then closed.
//
// Supported commands:
//   - "status"  → engine status and current focus
//   - "today"   → time per app today
//   - "date"    → time per app for a specific date
//   - "range"   → time per app for a date range
//   - "current" → what's currently focused

use crate::config::Config;
use crate::store::sqlite::Database;
use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Shared state that the tracker updates and the IPC server reads.
#[derive(Debug, Clone)]
pub struct CurrentState {
    pub focused_class: Option<String>,
    pub focused_title: Option<String>,
    pub focus_since: Option<DateTime<Local>>,
    pub session_start: Option<DateTime<Local>>,
    pub is_idle: bool,
    pub idle_since: Option<DateTime<Local>>,
}

impl CurrentState {
    pub fn new() -> Self {
        Self {
            focused_class: None,
            focused_title: None,
            focus_since: None,
            session_start: None,
            is_idle: false,
            idle_since: None,
        }
    }
}

// --- Request / Response types ---

#[derive(Debug, Deserialize)]
struct Request {
    command: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

#[derive(Debug, Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    fn success(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// Start the IPC server on a Unix socket.
///
/// This blocks and should be called from a dedicated thread.
pub fn run(
    config: &Config,
    shared_state: Arc<Mutex<CurrentState>>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let socket_path = config.ipc_socket_path();

    // Clean up stale socket from a previous crash
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    // Set non-blocking so we can check shutdown flag
    listener.set_nonblocking(true)?;

    log::info!("IPC server listening on: {}", socket_path.display());

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&shared_state);
                // Handle each client in a short-lived thread
                // (connections are one-shot request/response)
                let db_path = config.data_dir().join("index.db");
                thread::spawn(move || {
                    // Each client handler opens its own read-only connection
                    // to avoid sharing Connection across threads
                    match Database::open_readonly(&db_path) {
                        Ok(db) => {
                            if let Err(e) = handle_client(stream, &db, &state) {
                                log::warn!("IPC client error: {}", e);
                            }
                        }
                        Err(e) => {
                            log::warn!("IPC: failed to open DB for client: {}", e);
                        }
                    }
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No incoming connection — sleep briefly and retry
                thread::sleep(Duration::from_millis(200));
            }
            Err(e) => {
                log::warn!("IPC accept error: {}", e);
            }
        }
    }

    // Clean up socket file
    let _ = std::fs::remove_file(&socket_path);
    log::info!("IPC server shut down");
    Ok(())
}

/// Handle a single client connection: read request, query, respond.
fn handle_client(
    stream: UnixStream,
    db: &Database,
    shared_state: &Arc<Mutex<CurrentState>>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            write_response(&stream, Response::error(format!("Invalid request: {}", e)))?;
            return Ok(());
        }
    };

    let response = dispatch_command(&request, db, shared_state);
    write_response(&stream, response)?;

    Ok(())
}

/// Route a command to the appropriate handler.
fn dispatch_command(
    request: &Request,
    db: &Database,
    shared_state: &Arc<Mutex<CurrentState>>,
) -> Response {
    match request.command.as_str() {
        "status" => cmd_status(shared_state),
        "current" => cmd_current(shared_state),
        "today" => cmd_today(db),
        "date" => cmd_date(db, request),
        "range" => cmd_range(db, request),
        other => Response::error(format!(
            "Unknown command '{}'. Available: status, current, today, date, range",
            other
        )),
    }
}

fn cmd_status(shared_state: &Arc<Mutex<CurrentState>>) -> Response {
    let state = shared_state.lock().unwrap();
    Response::success(serde_json::json!({
        "running": true,
        "session_start": state.session_start.map(|t| t.to_rfc3339()),
        "is_idle": state.is_idle,
        "focused_class": state.focused_class,
    }))
}

fn cmd_current(shared_state: &Arc<Mutex<CurrentState>>) -> Response {
    let state = shared_state.lock().unwrap();
    let focus_duration_ms = state
        .focus_since
        .map(|start| (Local::now() - start).num_milliseconds());

    Response::success(serde_json::json!({
        "class": state.focused_class,
        "title": state.focused_title,
        "focus_since": state.focus_since.map(|t| t.to_rfc3339()),
        "focus_duration_ms": focus_duration_ms,
        "is_idle": state.is_idle,
    }))
}

fn cmd_today(db: &Database) -> Response {
    match db.query_today() {
        Ok(data) => Response::success(serde_json::to_value(data).unwrap()),
        Err(e) => Response::error(format!("Query failed: {}", e)),
    }
}

fn cmd_date(db: &Database, request: &Request) -> Response {
    let date = match &request.date {
        Some(d) => d,
        None => return Response::error("'date' field required for date command"),
    };
    match db.query_date(date) {
        Ok(data) => Response::success(serde_json::to_value(data).unwrap()),
        Err(e) => Response::error(format!("Query failed: {}", e)),
    }
}

fn cmd_range(db: &Database, request: &Request) -> Response {
    let from = match &request.from {
        Some(f) => f,
        None => return Response::error("'from' field required for range command"),
    };
    let to = match &request.to {
        Some(t) => t,
        None => return Response::error("'to' field required for range command"),
    };
    match db.query_range(from, to) {
        Ok(data) => Response::success(serde_json::to_value(data).unwrap()),
        Err(e) => Response::error(format!("Query failed: {}", e)),
    }
}

/// Write a JSON response and close the connection.
fn write_response(mut stream: &UnixStream, response: Response) -> Result<()> {
    let mut data = serde_json::to_string(&response)?;
    data.push('\n');
    stream.write_all(data.as_bytes())?;
    stream.flush()?;
    Ok(())
}
