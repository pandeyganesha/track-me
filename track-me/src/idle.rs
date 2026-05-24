// track-me: Idle detection
//
// Monitors for user inactivity by tracking time since the last
// compositor event. If no events are received for the configured
// timeout, an IdleStart event is emitted. When activity resumes
// (any new event), IdleEnd is emitted.
//
// Limitation: This is event-based idle detection. If the user is
// watching a video (no compositor events), they may be falsely
// marked as idle. Future improvement: check for active audio
// streams via PipeWire/PulseAudio.

use crate::config::IdleConfig;
use crate::event::Event;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Shared state for tracking the last event time.
///
/// The main event loop updates this on every event, and the idle
/// detector thread reads it to determine if the user is idle.
pub struct IdleState {
    pub last_event_time: Instant,
}

impl IdleState {
    pub fn new() -> Self {
        Self {
            last_event_time: Instant::now(),
        }
    }
}

/// Run the idle detector in a loop.
///
/// This should be called from a dedicated thread. It periodically
/// checks if enough time has passed since the last event and sends
/// IdleStart/IdleEnd events through the channel.
pub fn run(
    sender: mpsc::Sender<Event>,
    shutdown: Arc<AtomicBool>,
    config: IdleConfig,
    idle_state: Arc<Mutex<IdleState>>,
) {
    if !config.enabled {
        log::info!("Idle detection disabled");
        return;
    }

    let timeout = Duration::from_secs(config.timeout_secs);
    let check_interval = Duration::from_secs(30); // Check every 30 seconds
    let mut is_idle = false;

    log::info!(
        "Idle detection enabled (timeout: {}s)",
        config.timeout_secs
    );

    loop {
        thread::sleep(check_interval);

        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        let elapsed = {
            let state = idle_state.lock().unwrap();
            state.last_event_time.elapsed()
        };

        if elapsed >= timeout && !is_idle {
            // Transition: active → idle
            is_idle = true;
            if sender.send(Event::IdleStart).is_err() {
                break; // Channel closed
            }
            log::info!("Idle timeout reached ({:?} since last event)", elapsed);
        } else if elapsed < timeout && is_idle {
            // This shouldn't normally happen here (IdleEnd is typically
            // triggered by the main loop when a new event arrives), but
            // handle it for robustness.
            is_idle = false;
        }
    }

    log::info!("Idle detector shutting down");
}
