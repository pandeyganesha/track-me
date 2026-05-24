// track-me: Storage layer
//
// Dual-write persistence: every event is appended to a daily JSONL file
// (the immutable source of truth) and indexed in SQLite (for fast queries).

pub mod jsonl;
pub mod sqlite;

use crate::config::Config;
use crate::event::TimestampedEvent;
use anyhow::Result;
use std::path::PathBuf;

/// Unified storage handle that writes to both JSONL and SQLite.
pub struct Store {
    jsonl: jsonl::JsonlWriter,
    db: sqlite::Database,
    data_dir: PathBuf,
}

impl Store {
    /// Initialize storage, creating directories and database as needed.
    pub fn new(config: &Config) -> Result<Self> {
        let data_dir = config.data_dir();

        // Ensure directories exist
        let events_dir = data_dir.join("events");
        std::fs::create_dir_all(&events_dir)?;
        log::info!("Data directory: {}", data_dir.display());

        let jsonl = jsonl::JsonlWriter::new(events_dir)?;
        let db = sqlite::Database::new(&data_dir.join("index.db"))?;

        Ok(Self {
            jsonl,
            db,
            data_dir,
        })
    }

    /// Persist an event to both JSONL and SQLite.
    pub fn write_event(&mut self, event: &TimestampedEvent) -> Result<()> {
        // JSONL is the source of truth — write there first
        self.jsonl.append(event)?;
        // Then index in SQLite
        self.db.insert_event(event)?;
        Ok(())
    }

    /// Record a completed focus span in the SQLite index.
    pub fn write_focus_span(
        &mut self,
        start_ts: &chrono::DateTime<chrono::Local>,
        end_ts: &chrono::DateTime<chrono::Local>,
        duration_ms: i64,
        window_class: &str,
        window_title: &str,
        window_id: &str,
        idle: bool,
    ) -> Result<()> {
        self.db.insert_focus_span(
            start_ts,
            end_ts,
            duration_ms,
            window_class,
            window_title,
            window_id,
            idle,
        )
    }

    /// Get a read-only handle to the SQLite database for IPC queries.
    ///
    /// SQLite with WAL mode supports concurrent readers, so the IPC
    /// server can query while the tracker writes.
    pub fn open_reader(config: &Config) -> Result<sqlite::Database> {
        let db_path = config.data_dir().join("index.db");
        sqlite::Database::open_readonly(&db_path)
    }

    /// Returns the data directory path.
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}
