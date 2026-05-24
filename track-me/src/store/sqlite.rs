// track-me: SQLite queryable index
//
// This is a derived index built alongside the JSONL source of truth.
// It stores raw events and pre-computed focus spans for fast queries.
// If corrupted, it can be rebuilt from the JSONL files (future feature).
//
// Uses WAL mode for concurrent read access from the IPC server.

use crate::event::{Event, TimestampedEvent};
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;

/// SQLite database handle for the track-me index.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) the database in read-write mode.
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {}", path.display()))?;

        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    /// Open the database in read-only mode (for IPC queries).
    pub fn open_readonly(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("Failed to open database (readonly): {}", path.display()))?;

        Ok(Self { conn })
    }

    /// Create tables and indices if they don't exist.
    fn initialize(&self) -> Result<()> {
        // Enable WAL mode for concurrent readers
        self.conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        // Sync less frequently — we have JSONL as the source of truth
        self.conn.execute_batch("PRAGMA synchronous=NORMAL;")?;

        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp   TEXT    NOT NULL,
                event_type  TEXT    NOT NULL,
                class       TEXT,
                title       TEXT,
                window_id   TEXT,
                workspace   TEXT
            );

            CREATE TABLE IF NOT EXISTS focus_spans (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                start_ts      TEXT    NOT NULL,
                end_ts        TEXT    NOT NULL,
                duration_ms   INTEGER NOT NULL,
                window_class  TEXT    NOT NULL,
                window_title  TEXT    NOT NULL,
                window_id     TEXT    NOT NULL,
                idle          INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_events_timestamp
                ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_type
                ON events(event_type);
            CREATE INDEX IF NOT EXISTS idx_focus_spans_start
                ON focus_spans(start_ts);
            CREATE INDEX IF NOT EXISTS idx_focus_spans_class
                ON focus_spans(window_class);
            CREATE INDEX IF NOT EXISTS idx_focus_spans_date
                ON focus_spans(date(start_ts));
            ",
        )?;

        Ok(())
    }

    /// Insert a raw event into the events table.
    pub fn insert_event(&self, event: &TimestampedEvent) -> Result<()> {
        let ts = event.ts.to_rfc3339();
        let (event_type, class, title, window_id, workspace) = match &event.inner {
            Event::FocusChanged {
                class,
                title,
                window_id,
            } => (
                "focus_changed",
                Some(class.as_str()),
                Some(title.as_str()),
                Some(window_id.as_str()),
                None,
            ),
            Event::WindowOpened {
                class,
                title,
                window_id,
                workspace,
            } => (
                "window_opened",
                Some(class.as_str()),
                Some(title.as_str()),
                Some(window_id.as_str()),
                Some(workspace.as_str()),
            ),
            Event::WindowClosed { window_id } => (
                "window_closed",
                None,
                None,
                Some(window_id.as_str()),
                None,
            ),
            Event::TitleChanged { window_id, title } => (
                "title_changed",
                None,
                Some(title.as_str()),
                Some(window_id.as_str()),
                None,
            ),
            Event::WorkspaceChanged { id: _, name } => (
                "workspace_changed",
                None,
                None,
                None,
                Some(name.as_str()),
            ),
            Event::SessionStart => ("session_start", None, None, None, None),
            Event::SessionEnd => ("session_end", None, None, None, None),
            Event::IdleStart => ("idle_start", None, None, None, None),
            Event::IdleEnd => ("idle_end", None, None, None, None),
        };

        // For WorkspaceChanged, store the workspace id in window_id column
        // to avoid adding another column just for this
        let wid = match &event.inner {
            Event::WorkspaceChanged { id, .. } => Some(id.as_str()),
            _ => window_id,
        };

        self.conn.execute(
            "INSERT INTO events (timestamp, event_type, class, title, window_id, workspace)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![ts, event_type, class, title, wid, workspace],
        )?;

        Ok(())
    }

    /// Insert a completed focus span.
    pub fn insert_focus_span(
        &self,
        start_ts: &DateTime<Local>,
        end_ts: &DateTime<Local>,
        duration_ms: i64,
        window_class: &str,
        window_title: &str,
        window_id: &str,
        idle: bool,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO focus_spans (start_ts, end_ts, duration_ms, window_class, window_title, window_id, idle)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                start_ts.to_rfc3339(),
                end_ts.to_rfc3339(),
                duration_ms,
                window_class,
                window_title,
                window_id,
                idle as i32,
            ],
        )?;
        Ok(())
    }

    // --- Query methods (used by IPC server) ---

    /// Get total time per window class for today (non-idle spans only).
    pub fn query_today(&self) -> Result<HashMap<String, i64>> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        self.query_date(&today)
    }

    /// Get total time per window class for a specific date.
    pub fn query_date(&self, date: &str) -> Result<HashMap<String, i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT window_class, SUM(duration_ms)
             FROM focus_spans
             WHERE date(start_ts) = ?1 AND idle = 0
             GROUP BY window_class
             ORDER BY SUM(duration_ms) DESC",
        )?;

        let mut result = HashMap::new();
        let rows = stmt.query_map(params![date], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        for row in rows {
            let (class, ms) = row?;
            result.insert(class, ms);
        }

        Ok(result)
    }

    /// Get total time per window class for a date range.
    pub fn query_range(&self, from: &str, to: &str) -> Result<HashMap<String, i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT window_class, SUM(duration_ms)
             FROM focus_spans
             WHERE date(start_ts) >= ?1 AND date(start_ts) <= ?2 AND idle = 0
             GROUP BY window_class
             ORDER BY SUM(duration_ms) DESC",
        )?;

        let mut result = HashMap::new();
        let rows = stmt.query_map(params![from, to], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        for row in rows {
            let (class, ms) = row?;
            result.insert(class, ms);
        }

        Ok(result)
    }
}
