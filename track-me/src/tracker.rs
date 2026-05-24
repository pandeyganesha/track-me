// track-me: Core tracker state machine
//
// The tracker is the heart of the engine. It receives normalized events,
// maintains the current focus state, and produces focus spans with
// accurate durations. It writes everything through the Store.
//
// State machine:
//   Active(window) --FocusChanged--> Active(new_window)  [emits span for old]
//   Active(window) --IdleStart-->    Idle(window)         [emits span for old]
//   Idle(window)   --IdleEnd-->      Active(window)       [resumes tracking]
//   Idle(window)   --FocusChanged--> Active(new_window)   [implicitly ends idle]
//   *              --SessionEnd-->   Stopped              [emits final span]

use crate::event::{Event, TimestampedEvent};
use crate::ipc::CurrentState;
use crate::store::Store;
use anyhow::Result;
use chrono::{DateTime, Local};
use std::sync::{Arc, Mutex};

/// Information about the currently focused window.
#[derive(Debug, Clone)]
struct FocusInfo {
    class: String,
    title: String,
    window_id: String,
    /// When this window gained focus.
    start: DateTime<Local>,
}

/// Core tracker state machine.
pub struct Tracker {
    store: Store,
    /// Currently focused window (None before first focus event).
    current_focus: Option<FocusInfo>,
    /// Whether the user is currently idle.
    is_idle: bool,
    /// When the session started.
    session_start: Option<DateTime<Local>>,
    /// Shared state readable by the IPC server.
    shared_state: Arc<Mutex<CurrentState>>,
}

impl Tracker {
    pub fn new(store: Store, shared_state: Arc<Mutex<CurrentState>>) -> Self {
        Self {
            store,
            current_focus: None,
            is_idle: false,
            session_start: None,
            shared_state,
        }
    }

    /// Record session start.
    pub fn start(&mut self) -> Result<()> {
        let now = Local::now();
        self.session_start = Some(now);

        let event = TimestampedEvent::now(Event::SessionStart);
        self.store.write_event(&event)?;

        // Update shared state
        if let Ok(mut state) = self.shared_state.lock() {
            state.session_start = Some(now);
        }

        log::info!("Session started at {}", now.to_rfc3339());
        Ok(())
    }

    /// Record session end and flush the final focus span.
    pub fn stop(&mut self) -> Result<()> {
        let now = Local::now();

        // Close the current focus span
        self.close_current_span(now)?;

        let event = TimestampedEvent::now(Event::SessionEnd);
        self.store.write_event(&event)?;

        log::info!("Session ended at {}", now.to_rfc3339());
        Ok(())
    }

    /// Process an incoming event through the state machine.
    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        let now = Local::now();
        let ts_event = TimestampedEvent { ts: now, inner: event.clone() };

        // Persist the raw event
        self.store.write_event(&ts_event)?;

        match event {
            Event::FocusChanged {
                class,
                title,
                window_id,
            } => {
                // If we were idle, the focus change implicitly ends idle
                if self.is_idle {
                    self.is_idle = false;
                    log::info!("Idle ended (focus changed)");
                }

                // Close the previous focus span
                self.close_current_span(now)?;

                // Start tracking the new window
                self.current_focus = Some(FocusInfo {
                    class: class.clone(),
                    title: title.clone(),
                    window_id: window_id.clone(),
                    start: now,
                });

                // Update shared state for IPC
                if let Ok(mut state) = self.shared_state.lock() {
                    state.focused_class = Some(class);
                    state.focused_title = Some(title);
                    state.focus_since = Some(now);
                    state.is_idle = false;
                }

                log::debug!("Focus → {}", self.current_focus.as_ref().unwrap().class);
            }

            Event::IdleStart => {
                if !self.is_idle {
                    self.is_idle = true;
                    // Close the current active span (up to idle start)
                    self.close_current_span(now)?;

                    // Re-open the focus info with the new timestamp,
                    // but mark that we're idle so the span will be tagged
                    if let Some(ref focus) = self.current_focus.clone() {
                        self.current_focus = Some(FocusInfo {
                            start: now,
                            ..focus.clone()
                        });
                    }

                    if let Ok(mut state) = self.shared_state.lock() {
                        state.is_idle = true;
                        state.idle_since = Some(now);
                    }

                    log::info!("User went idle");
                }
            }

            Event::IdleEnd => {
                if self.is_idle {
                    self.is_idle = false;

                    // Close the idle span
                    self.close_current_span_as_idle(now)?;

                    // Resume tracking the same window
                    if let Some(ref focus) = self.current_focus.clone() {
                        self.current_focus = Some(FocusInfo {
                            start: now,
                            ..focus.clone()
                        });
                    }

                    if let Ok(mut state) = self.shared_state.lock() {
                        state.is_idle = false;
                        state.idle_since = None;
                    }

                    log::info!("User returned from idle");
                }
            }

            // These events are stored in the event log but don't
            // affect the focus state machine directly.
            Event::WindowOpened { .. }
            | Event::WindowClosed { .. }
            | Event::TitleChanged { .. }
            | Event::WorkspaceChanged { .. } => {}

            // Session events are handled by start()/stop()
            Event::SessionStart | Event::SessionEnd => {}
        }

        Ok(())
    }

    /// Close the current focus span and write it to the store.
    fn close_current_span(&mut self, now: DateTime<Local>) -> Result<()> {
        if let Some(ref focus) = self.current_focus {
            let duration_ms = (now - focus.start).num_milliseconds();

            // Only write spans with positive duration
            if duration_ms > 0 {
                self.store.write_focus_span(
                    &focus.start,
                    &now,
                    duration_ms,
                    &focus.class,
                    &focus.title,
                    &focus.window_id,
                    false, // not idle
                )?;
            }
        }
        Ok(())
    }

    /// Close the current span, marking it as idle time.
    fn close_current_span_as_idle(&mut self, now: DateTime<Local>) -> Result<()> {
        if let Some(ref focus) = self.current_focus {
            let duration_ms = (now - focus.start).num_milliseconds();

            if duration_ms > 0 {
                self.store.write_focus_span(
                    &focus.start,
                    &now,
                    duration_ms,
                    &focus.class,
                    &focus.title,
                    &focus.window_id,
                    true, // idle
                )?;
            }
        }
        Ok(())
    }
}
