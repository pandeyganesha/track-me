// track-me: JSONL (JSON Lines) append-only event log
//
// Each day gets its own file: `YYYY-MM-DD.jsonl`
// Each line is a self-contained JSON object (one TimestampedEvent).
// Files are append-only and never modified — this is the immutable
// source of truth from which all other state can be reconstructed.

use crate::event::TimestampedEvent;
use anyhow::{Context, Result};
use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Appends serialized events to daily JSONL files.
pub struct JsonlWriter {
    events_dir: PathBuf,
    /// Currently open file and its date string, cached to avoid
    /// reopening the same file on every write.
    current: Option<(String, File)>,
}

impl JsonlWriter {
    pub fn new(events_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            events_dir,
            current: None,
        })
    }

    /// Append a single event to today's JSONL file.
    pub fn append(&mut self, event: &TimestampedEvent) -> Result<()> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let file = self.get_or_open_file(&today)?;

        let mut line = serde_json::to_string(event)
            .context("Failed to serialize event")?;
        line.push('\n');

        file.write_all(line.as_bytes())
            .context("Failed to write to JSONL file")?;
        // Flush immediately — crash safety is more important than throughput
        file.flush()
            .context("Failed to flush JSONL file")?;

        Ok(())
    }

    /// Get the file handle for the given date, opening a new file
    /// if the date changed (midnight rollover).
    fn get_or_open_file(&mut self, date: &str) -> Result<&mut File> {
        // Check if we already have the right file open
        let needs_new = match &self.current {
            Some((current_date, _)) => current_date != date,
            None => true,
        };

        if needs_new {
            let path = self.events_dir.join(format!("{}.jsonl", date));
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .with_context(|| format!("Failed to open {}", path.display()))?;
            log::info!("Opened event log: {}", path.display());
            self.current = Some((date.to_string(), file));
        }

        Ok(&mut self.current.as_mut().unwrap().1)
    }
}
