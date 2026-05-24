// track-me: Hyprland event provider
//
// Connects to Hyprland's `.socket2.sock` (the event broadcast socket)
// and translates compositor events into the common `Event` type.
//
// Hyprland event format: `EVENT_NAME>>DATA\n`
// where DATA format varies per event type.
//
// Reference: https://wiki.hyprland.org/IPC/

use crate::event::Event;
use crate::provider::EventProvider;
use anyhow::{Context, Result};
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use std::{env, thread};

/// Hyprland compositor event provider.
///
/// Reads from Hyprland's socket2 event stream and emits normalized events.
pub struct HyprlandProvider {
    socket_path: PathBuf,
}

impl HyprlandProvider {
    /// Create a new provider by resolving the Hyprland socket path
    /// from environment variables.
    pub fn new() -> Result<Self> {
        let socket_path = Self::resolve_socket_path()
            .context("Failed to resolve Hyprland socket path")?;
        Ok(Self { socket_path })
    }

    /// Build the socket path from `$XDG_RUNTIME_DIR` and
    /// `$HYPRLAND_INSTANCE_SIGNATURE`.
    fn resolve_socket_path() -> Result<PathBuf> {
        let instance = env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .context("HYPRLAND_INSTANCE_SIGNATURE not set — is Hyprland running?")?;
        let runtime_dir = env::var("XDG_RUNTIME_DIR")
            .context("XDG_RUNTIME_DIR not set")?;

        let path = PathBuf::from(runtime_dir)
            .join("hypr")
            .join(&instance)
            .join(".socket2.sock");

        if !path.exists() {
            anyhow::bail!("Hyprland socket not found at {}", path.display());
        }

        Ok(path)
    }

    /// Connect to the socket with retry logic.
    fn connect(&self) -> Result<UnixStream> {
        log::info!("Connecting to Hyprland socket: {}", self.socket_path.display());
        let stream = UnixStream::connect(&self.socket_path)
            .with_context(|| format!("Failed to connect to {}", self.socket_path.display()))?;
        // Set a read timeout so we can check the shutdown flag periodically
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        Ok(stream)
    }

    /// Parse a single line from the Hyprland event stream.
    ///
    /// Returns `Some(Event)` for events we care about, `None` for
    /// events we intentionally skip (noise).
    fn parse_line(line: &str) -> Option<Event> {
        // Format: "EVENT_NAME>>DATA"
        let (event_name, data) = line.split_once(">>")?;

        match event_name {
            // activewindow>>CLASS,TITLE
            "activewindow" => {
                let (class, title) = data.split_once(',')?;
                Some(Event::FocusChanged {
                    class: if class.is_empty() {
                        "desktop".to_string()
                    } else {
                        class.to_string()
                    },
                    title: title.to_string(),
                    // activewindow doesn't include the window ID;
                    // we could correlate with activewindowv2 but keeping
                    // it simple for now.
                    window_id: String::new(),
                })
            }

            // activewindowv2>>WINDOW_ID
            // We use this to fill in the window_id for the previous
            // FocusChanged event. However, since these always come in
            // pairs (activewindow then activewindowv2), we handle this
            // by emitting a FocusChanged only from activewindowv2 to
            // avoid duplicates.
            // Actually, let's keep the approach simple: emit from
            // activewindow (which has class+title) and skip v2.
            "activewindowv2" => None,

            // openwindow>>WINDOW_ID,WORKSPACE_ID,CLASS,TITLE
            "openwindow" => {
                let parts: Vec<&str> = data.splitn(4, ',').collect();
                if parts.len() >= 4 {
                    Some(Event::WindowOpened {
                        window_id: parts[0].to_string(),
                        workspace: parts[1].to_string(),
                        class: parts[2].to_string(),
                        title: parts[3].to_string(),
                    })
                } else {
                    log::warn!("Malformed openwindow event: {}", data);
                    None
                }
            }

            // closewindow>>WINDOW_ID
            "closewindow" => Some(Event::WindowClosed {
                window_id: data.to_string(),
            }),

            // windowtitlev2>>WINDOW_ID,NEW_TITLE
            "windowtitlev2" => {
                let (window_id, title) = data.split_once(',')?;
                Some(Event::TitleChanged {
                    window_id: window_id.to_string(),
                    title: title.to_string(),
                })
            }

            // workspacev2>>ID,NAME
            "workspacev2" => {
                let (id, name) = data.split_once(',')?;
                Some(Event::WorkspaceChanged {
                    id: id.to_string(),
                    name: name.to_string(),
                })
            }

            // Skip v1 variants (we use v2 for richer data) and
            // events that don't contribute to usage statistics.
            "workspace" | "windowtitle" | "focusedmon" | "focusedmonv2"
            | "createworkspace" | "createworkspacev2"
            | "destroyworkspace" | "destroyworkspacev2"
            | "fullscreen" | "openlayer" | "closelayer" | "urgent" => None,

            _ => {
                log::trace!("Ignoring unknown Hyprland event: {}", event_name);
                None
            }
        }
    }
}

impl EventProvider for HyprlandProvider {
    fn name(&self) -> &'static str {
        "hyprland"
    }

    fn run(
        self: Box<Self>,
        sender: mpsc::Sender<Event>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()> {
        let mut backoff_secs = 1u64;

        loop {
            if shutdown.load(Ordering::Relaxed) {
                log::info!("Hyprland provider shutting down");
                return Ok(());
            }

            match self.connect() {
                Ok(stream) => {
                    backoff_secs = 1; // Reset backoff on successful connection
                    let reader = BufReader::new(stream);

                    for line_result in reader.lines() {
                        if shutdown.load(Ordering::Relaxed) {
                            return Ok(());
                        }

                        match line_result {
                            Ok(line) => {
                                if let Some(event) = Self::parse_line(&line) {
                                    if sender.send(event).is_err() {
                                        // Receiver dropped — engine is shutting down
                                        return Ok(());
                                    }
                                }
                            }
                            Err(e) => {
                                if e.kind() == std::io::ErrorKind::WouldBlock
                                    || e.kind() == std::io::ErrorKind::TimedOut
                                {
                                    // Read timeout — just loop back to check shutdown flag
                                    continue;
                                }
                                log::warn!("Socket read error: {}, will reconnect", e);
                                break; // Break inner loop to reconnect
                            }
                        }
                    }

                    log::info!("Hyprland socket stream ended, will reconnect");
                }
                Err(e) => {
                    log::warn!(
                        "Failed to connect to Hyprland (retrying in {}s): {}",
                        backoff_secs, e
                    );
                }
            }

            // Exponential backoff for reconnection (max 30s)
            thread::sleep(Duration::from_secs(backoff_secs));
            backoff_secs = (backoff_secs * 2).min(30);
        }
    }
}
