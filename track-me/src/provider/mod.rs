// track-me: Event provider abstraction
//
// The `EventProvider` trait is the boundary between compositor-specific
// event sources and the rest of the engine. Each provider (Hyprland,
// Sway, X11, etc.) implements this trait to translate its native event
// stream into the common `Event` type.

pub mod hyprland;
pub mod sway;

use crate::config::Config;
use crate::event::Event;
use anyhow::{bail, Result};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};

/// A source of compositor/WM events.
///
/// Implementations connect to a compositor's event stream, parse
/// native events, and send normalized `Event` values through the
/// provided channel. The `run` method blocks until the shutdown
/// flag is set or the event source disconnects.
pub trait EventProvider: Send {
    /// Human-readable name for logging (e.g. "hyprland").
    fn name(&self) -> &'static str;

    /// Block and stream events until shutdown is requested.
    ///
    /// Must handle reconnection internally if the event source
    /// disconnects unexpectedly.
    fn run(
        self: Box<Self>,
        sender: mpsc::Sender<Event>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()>;
}

/// Auto-detect the running compositor from environment variables
/// and return the appropriate provider.
///
/// Detection order:
/// 1. `$HYPRLAND_INSTANCE_SIGNATURE` → Hyprland
/// 2. `$SWAYSOCK` or `$I3SOCK` → Sway/i3
/// 3. (future) `$DISPLAY` → X11
pub fn detect_provider(config: &Config) -> Result<Box<dyn EventProvider>> {
    let provider_name = &config.general.provider;

    match provider_name.as_str() {
        "hyprland" => Ok(Box::new(hyprland::HyprlandProvider::new()?)),
        "sway" | "i3" => Ok(Box::new(sway::SwayProvider::new()?)),

        "auto" => {
            // Try providers in order of preference
            if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
                log::info!("Auto-detected Hyprland compositor");
                return Ok(Box::new(hyprland::HyprlandProvider::new()?));
            }
            
            if std::env::var("SWAYSOCK").is_ok() || std::env::var("I3SOCK").is_ok() {
                log::info!("Auto-detected Sway/i3 compositor");
                return Ok(Box::new(sway::SwayProvider::new()?));
            }

            // Future: check $DISPLAY for X11

            bail!(
                "Could not auto-detect compositor. \
                 Set 'provider' in config or ensure compositor env vars are set."
            );
        }

        other => bail!("Unknown provider '{}'. Supported: hyprland, sway, i3, auto", other),
    }
}
