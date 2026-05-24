// track-me: Event type definitions
//
// All compositor/WM events are normalized into this common format.
// This is the abstraction boundary between provider-specific protocols
// and the rest of the engine. Every event is timestamped and serialized
// to JSONL as the immutable source of truth.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// A compositor/system event normalized into a provider-agnostic form.
///
/// Each variant carries only the data relevant to that event type.
/// The `serde(tag = "event", content = "data")` layout produces compact
/// JSON like: `{"event":"focus_changed","data":{"class":"kitty",...}}`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum Event {
    /// The focused window changed.
    #[serde(rename = "focus_changed")]
    FocusChanged {
        /// Window class (e.g. "kitty", "code", "firefox")
        class: String,
        /// Window title (e.g. "main.rs - track-me - Visual Studio Code")
        title: String,
        /// Compositor-assigned window identifier
        window_id: String,
    },

    /// A new window was created.
    #[serde(rename = "window_opened")]
    WindowOpened {
        class: String,
        title: String,
        window_id: String,
        workspace: String,
    },

    /// A window was destroyed.
    #[serde(rename = "window_closed")]
    WindowClosed { window_id: String },

    /// A window's title changed (e.g. switched tabs in a browser).
    #[serde(rename = "title_changed")]
    TitleChanged {
        window_id: String,
        title: String,
    },

    /// The active workspace changed.
    #[serde(rename = "workspace_changed")]
    WorkspaceChanged { id: String, name: String },

    /// Tracking session started (engine boot).
    #[serde(rename = "session_start")]
    SessionStart,

    /// Tracking session ended (clean shutdown).
    #[serde(rename = "session_end")]
    SessionEnd,

    /// User went idle (no input for configured duration).
    #[serde(rename = "idle_start")]
    IdleStart,

    /// User returned from idle.
    #[serde(rename = "idle_end")]
    IdleEnd,
}

/// An event paired with its wall-clock timestamp.
///
/// This is the unit of persistence — each line in the JSONL log
/// is one serialized `TimestampedEvent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedEvent {
    /// ISO-8601 timestamp with timezone
    pub ts: DateTime<Local>,
    /// The event payload
    #[serde(flatten)]
    pub inner: Event,
}

impl TimestampedEvent {
    /// Wrap an event with the current wall-clock time.
    pub fn now(event: Event) -> Self {
        Self {
            ts: Local::now(),
            inner: event,
        }
    }
}
