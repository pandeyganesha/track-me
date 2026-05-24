// track-me: Core engine entry point
//
// Orchestrates the event pipeline:
//   Provider (compositor events)  ─┐
//   Idle detector (timeout events) ─┼──▶  Tracker  ──▶  Store (JSONL + SQLite)
//   IPC server (query interface)   ─┘
//
// All threads coordinate through a shared shutdown flag (AtomicBool)
// and communicate via mpsc channels.

mod config;
mod event;
mod idle;
mod ipc;
mod provider;
mod store;
mod tracker;

use crate::event::Event;
use crate::idle::IdleState;
use crate::ipc::CurrentState;
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    // Initialize logging (respects RUST_LOG env var, defaults to "info")
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .format_timestamp_secs()
    .init();

    log::info!("track-me engine starting");

    // --- Load configuration ---
    let config = config::Config::load()
        .context("Failed to load configuration")?;
    log::info!("Provider: {}", config.general.provider);

    // --- Initialize storage ---
    let store = store::Store::new(&config)
        .context("Failed to initialize storage")?;

    // --- Detect compositor provider ---
    let event_provider = provider::detect_provider(&config)
        .context("Failed to initialize event provider")?;
    log::info!("Provider ready: {}", event_provider.name());

    // --- Shared state ---
    let shutdown = Arc::new(AtomicBool::new(false));
    let shared_state = Arc::new(Mutex::new(CurrentState::new()));
    let idle_state = Arc::new(Mutex::new(IdleState::new()));

    // --- Signal handler (SIGINT / SIGTERM) ---
    let shutdown_signal = Arc::clone(&shutdown);
    ctrlc::set_handler(move || {
        log::info!("Shutdown signal received (SIGINT/SIGTERM)");
        shutdown_signal.store(true, Ordering::SeqCst);
    })
    .context("Failed to set signal handler")?;

    // --- Event channel ---
    let (tx, rx) = mpsc::channel::<Event>();

    // --- Spawn provider thread ---
    let provider_tx = tx.clone();
    let provider_shutdown = Arc::clone(&shutdown);
    let provider_thread = thread::Builder::new()
        .name("provider".into())
        .spawn(move || {
            if let Err(e) = event_provider.run(provider_tx, provider_shutdown) {
                log::error!("Provider thread error: {}", e);
            }
        })
        .context("Failed to spawn provider thread")?;

    // --- Spawn idle detector thread ---
    let idle_tx = tx.clone();
    let idle_shutdown = Arc::clone(&shutdown);
    let idle_config = config.idle.clone();
    let idle_state_clone = Arc::clone(&idle_state);
    let idle_thread = thread::Builder::new()
        .name("idle-detector".into())
        .spawn(move || {
            idle::run(idle_tx, idle_shutdown, idle_config, idle_state_clone);
        })
        .context("Failed to spawn idle detector thread")?;

    // --- Spawn IPC server thread ---
    let ipc_state = Arc::clone(&shared_state);
    let ipc_shutdown = Arc::clone(&shutdown);
    let ipc_config = config.clone();
    let ipc_thread = thread::Builder::new()
        .name("ipc-server".into())
        .spawn(move || {
            if let Err(e) = ipc::run(&ipc_config, ipc_state, ipc_shutdown) {
                log::error!("IPC server error: {}", e);
            }
        })
        .context("Failed to spawn IPC server thread")?;

    // Drop the sender clone so the channel closes when all producers exit
    drop(tx);

    // --- Main loop: tracker ---
    let mut tracker_engine = tracker::Tracker::new(store, shared_state);
    tracker_engine.start()?;

    // Track whether we're currently idle for the IdleEnd injection logic
    let mut currently_idle = false;

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(event) => {
                // Update the last-event timestamp for idle detection
                if let Ok(mut state) = idle_state.lock() {
                    state.last_event_time = Instant::now();
                }

                // If we receive a real compositor event while idle,
                // inject an IdleEnd before processing the event
                let is_idle_event = matches!(event, Event::IdleStart | Event::IdleEnd);
                if currently_idle && !is_idle_event {
                    tracker_engine.handle_event(Event::IdleEnd)?;
                    currently_idle = false;
                }

                // Track idle state
                match &event {
                    Event::IdleStart => currently_idle = true,
                    Event::IdleEnd => currently_idle = false,
                    _ => {}
                }

                tracker_engine.handle_event(event)?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                log::warn!("All event producers disconnected");
                break;
            }
        }
    }

    // --- Clean shutdown ---
    log::info!("Shutting down...");
    tracker_engine.stop()?;

    // Wait for threads (with timeout)
    let _ = provider_thread.join();
    let _ = idle_thread.join();
    let _ = ipc_thread.join();

    log::info!("track-me engine stopped");
    Ok(())
}