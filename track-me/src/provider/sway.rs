// track-me: Sway and i3 event provider
//
// Connects to Sway or i3's IPC socket to receive window events.
// It parses the JSON payload provided by the compositor and translates
// it into our common `Event` type.

use crate::event::Event;
use crate::provider::EventProvider;
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use std::thread;
use swayipc::{Connection, EventType, Fallible, WindowChange, WorkspaceChange};

/// Sway/i3 compositor event provider.
///
/// Subscribes to window and workspace events from the compositor.
pub struct SwayProvider {
    // We don't store the connection here because we need to establish it
    // on the background thread inside `run()`.
}

impl SwayProvider {
    pub fn new() -> Result<Self> {
        // Just verify that the env var exists as a sanity check.
        // If neither SWAYSOCK nor I3SOCK are set, and we are not in auto mode,
        // we might fail later, but that's fine.
        Ok(Self {})
    }

    /// Convert a swayipc Event into our normalized Event format.
    fn translate_event(event: swayipc::Event) -> Option<Event> {
        match event {
            swayipc::Event::Window(w) => {
                let window_id = w.container.id.to_string();
                let class = w
                    .container
                    .app_id // Wayland uses app_id
                    .or(w.container.window_properties.and_then(|p| p.class)) // X11 uses window class
                    .unwrap_or_else(|| "desktop".to_string());
                
                let title = w.container.name.unwrap_or_default();

                match w.change {
                    WindowChange::Focus => Some(Event::FocusChanged {
                        class,
                        title,
                        window_id,
                    }),
                    WindowChange::New => Some(Event::WindowOpened {
                        class,
                        title,
                        window_id,
                        workspace: "unknown".to_string(), // Sway doesn't include workspace in the window event directly
                    }),
                    WindowChange::Close => Some(Event::WindowClosed { window_id }),
                    WindowChange::Title => Some(Event::TitleChanged {
                        window_id,
                        title,
                    }),
                    _ => None,
                }
            }
            swayipc::Event::Workspace(ws) => {
                if ws.change == WorkspaceChange::Focus {
                    Some(Event::WorkspaceChanged {
                        id: ws.current.as_ref().map(|c| c.id.to_string()).unwrap_or_default(),
                        name: ws.current.as_ref().and_then(|c| c.name.clone()).unwrap_or_default(),
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl EventProvider for SwayProvider {
    fn name(&self) -> &'static str {
        "sway"
    }

    fn run(
        self: Box<Self>,
        sender: mpsc::Sender<Event>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()> {
        let mut backoff_secs = 1u64;

        loop {
            if shutdown.load(Ordering::Relaxed) {
                log::info!("Sway provider shutting down");
                return Ok(());
            }

            log::info!("Attempting to connect to Sway/i3 IPC...");
            match Connection::new() {
                Ok(mut conn) => {
                    backoff_secs = 1; // Reset backoff
                    
                    // First, get the currently focused window so we start tracking immediately
                    if let Ok(tree) = conn.get_tree() {
                        if let Some(focused) = tree.find_focused_as_ref(|_| true) {
                            let class = focused
                                .app_id
                                .clone()
                                .or_else(|| focused.window_properties.as_ref().and_then(|p| p.class.clone()))
                                .unwrap_or_else(|| "desktop".to_string());
                                
                            let event = Event::FocusChanged {
                                class,
                                title: focused.name.clone().unwrap_or_default(),
                                window_id: focused.id.to_string(),
                            };
                            let _ = sender.send(event);
                        }
                    }

                    // Subscribe to window and workspace events
                    match conn.subscribe(vec![EventType::Window, EventType::Workspace]) {
                        Ok(mut stream) => {
                            log::info!("Successfully subscribed to Sway/i3 events");
                            
                            // The stream is blocking. To check the shutdown flag, we have to rely
                            // on the socket closing, or the process being killed.
                            // However, we can use try_next or set a timeout if we used tokio.
                            // With synchronous `swayipc`, it blocks. So we will just loop.
                            for event_res in stream {
                                if shutdown.load(Ordering::Relaxed) {
                                    return Ok(());
                                }

                                match event_res {
                                    Ok(event) => {
                                        if let Some(normalized) = Self::translate_event(event) {
                                            if sender.send(normalized).is_err() {
                                                return Ok(()); // Receiver dropped
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("Sway IPC read error: {}, will reconnect", e);
                                        break; // Break to reconnect
                                    }
                                }
                            }
                            
                            log::info!("Sway IPC stream ended, will reconnect");
                        }
                        Err(e) => {
                            log::error!("Failed to subscribe to Sway IPC: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to connect to Sway (retrying in {}s): {}",
                        backoff_secs, e
                    );
                }
            }

            // Exponential backoff for reconnection
            thread::sleep(Duration::from_secs(backoff_secs));
            backoff_secs = (backoff_secs * 2).min(30);
        }
    }
}
