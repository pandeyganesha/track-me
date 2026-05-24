// track-me: X11 event provider
//
// Connects to the X server and listens for changes to the _NET_ACTIVE_WINDOW
// property on the root window. Extracts WM_CLASS and _NET_WM_NAME to populate
// our common `Event` type.

use crate::event::Event;
use crate::provider::EventProvider;
use anyhow::{bail, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::protocol::xproto::{AtomEnum, ChangeWindowAttributesAux, EventMask};
use x11rb::protocol::Event as XEvent;

pub struct X11Provider {}

impl X11Provider {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    fn get_active_window(conn: &impl Connection, root: u32, net_active_window: u32) -> Result<u32> {
        let cookie = conn.get_property(
            false,
            root,
            net_active_window,
            0u32, // AnyPropertyType
            0,
            1,
        )?;
        let reply = cookie.reply()?;

        if let Some(mut val) = reply.value32() {
            if let Some(window) = val.next() {
                if window > 0 {
                    return Ok(window);
                }
            }
        }
        bail!("No active window")
    }

    fn get_window_class(conn: &impl Connection, window: u32, wm_class: u32) -> String {
        if let Ok(cookie) = conn.get_property(false, window, wm_class, 0u32, 0, 1024) {
            if let Ok(reply) = cookie.reply() {
                let bytes = reply.value;
                let strings: Vec<&[u8]> = bytes.split(|&b| b == 0).collect();
                // Usually the second string is the class (general), the first is the instance.
                if strings.len() > 1 && !strings[1].is_empty() {
                    return String::from_utf8_lossy(strings[1]).to_string();
                } else if !strings.is_empty() && !strings[0].is_empty() {
                    return String::from_utf8_lossy(strings[0]).to_string();
                }
            }
        }
        "desktop".to_string()
    }

    fn get_window_title(conn: &impl Connection, window: u32, net_wm_name: u32) -> String {
        // Try _NET_WM_NAME first
        if let Ok(cookie) = conn.get_property(false, window, net_wm_name, 0u32, 0, 1024) {
            if let Ok(reply) = cookie.reply() {
                if !reply.value.is_empty() {
                    return String::from_utf8_lossy(&reply.value).to_string();
                }
            }
        }
        // Fallback to WM_NAME
        if let Ok(cookie) = conn.get_property(false, window, u32::from(AtomEnum::WM_NAME), 0u32, 0, 1024) {
            if let Ok(reply) = cookie.reply() {
                if !reply.value.is_empty() {
                    return String::from_utf8_lossy(&reply.value).to_string();
                }
            }
        }
        "".to_string()
    }
}

impl EventProvider for X11Provider {
    fn name(&self) -> &'static str {
        "x11"
    }

    fn run(
        self: Box<Self>,
        sender: mpsc::Sender<Event>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()> {
        let mut backoff_secs = 1u64;

        loop {
            if shutdown.load(Ordering::Relaxed) {
                log::info!("X11 provider shutting down");
                return Ok(());
            }

            log::info!("Attempting to connect to X11 server...");
            match x11rb::connect(None) {
                Ok((conn, screen_num)) => {
                    backoff_secs = 1;
                    let root = conn.setup().roots[screen_num].root;

                    // Fetch Atoms
                    let net_active_window = match conn.intern_atom(false, b"_NET_ACTIVE_WINDOW") {
                        Ok(cookie) => cookie.reply().map(|r| r.atom).unwrap_or(0),
                        Err(_) => 0,
                    };
                    let net_wm_name = match conn.intern_atom(false, b"_NET_WM_NAME") {
                        Ok(cookie) => cookie.reply().map(|r| r.atom).unwrap_or(0),
                        Err(_) => 0,
                    };
                    let wm_class = match conn.intern_atom(false, b"WM_CLASS") {
                        Ok(cookie) => cookie.reply().map(|r| r.atom).unwrap_or(0),
                        Err(_) => 0,
                    };

                    if net_active_window == 0 {
                        log::error!("_NET_ACTIVE_WINDOW atom not supported by this X11 WM");
                        thread::sleep(Duration::from_secs(5));
                        continue;
                    }

                    // Send the initial active window state
                    if let Ok(window_id) = Self::get_active_window(&conn, root, net_active_window) {
                        let class = Self::get_window_class(&conn, window_id, wm_class);
                        let title = Self::get_window_title(&conn, window_id, net_wm_name);
                        let _ = sender.send(Event::FocusChanged {
                            class,
                            title,
                            window_id: window_id.to_string(),
                        });
                    }

                    // Subscribe to PropertyChange events on the root window
                    let attrs = ChangeWindowAttributesAux::new()
                        .event_mask(EventMask::PROPERTY_CHANGE);
                    if let Err(e) = conn.change_window_attributes(root, &attrs) {
                        log::error!("Failed to subscribe to root window property changes: {}", e);
                    }
                    let _ = conn.flush();

                    log::info!("Successfully subscribed to X11 root window events");

                    // Event polling loop
                    loop {
                        if shutdown.load(Ordering::Relaxed) {
                            return Ok(());
                        }

                        match conn.poll_for_event() {
                            Ok(Some(event)) => {
                                if let XEvent::PropertyNotify(prop_event) = event {
                                    if prop_event.window == root && prop_event.atom == net_active_window {
                                        if let Ok(window_id) =
                                            Self::get_active_window(&conn, root, net_active_window)
                                        {
                                            let class = Self::get_window_class(&conn, window_id, wm_class);
                                            let title = Self::get_window_title(&conn, window_id, net_wm_name);
                                            let _ = sender.send(Event::FocusChanged {
                                                class,
                                                title,
                                                window_id: window_id.to_string(),
                                            });
                                        } else {
                                            // Focus lost / desktop focused
                                            let _ = sender.send(Event::FocusChanged {
                                                class: "desktop".to_string(),
                                                title: "".to_string(),
                                                window_id: "0".to_string(),
                                            });
                                        }
                                    }
                                }
                            }
                            Ok(None) => {
                                // No event available, sleep briefly to prevent 100% CPU usage
                                thread::sleep(Duration::from_millis(50));
                            }
                            Err(e) => {
                                log::warn!("X11 connection error: {}", e);
                                break; // Break out of polling loop to reconnect
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to connect to X11 (retrying in {}s): {:?}",
                        backoff_secs, e
                    );
                }
            }

            thread::sleep(Duration::from_secs(backoff_secs));
            backoff_secs = (backoff_secs * 2).min(30);
        }
    }
}
