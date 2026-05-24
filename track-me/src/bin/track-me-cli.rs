use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Attribute, Cell, Color, Table};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "track-me-cli")]
#[command(version)]
#[command(about = "CLI dashboard for the track-me activity tracker")]
#[command(long_about = "
The track-me-cli tool connects to your locally running track-me background engine
to fetch and display your computer usage statistics. 

Ensure the engine is running via systemd:
    systemctl --user status track-me.service
")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show current status and active window
    #[command(about = "Check if the engine is running and what it is currently tracking")]
    Status,
    
    /// Show today's usage statistics
    #[command(about = "Display a summary of all applications used today")]
    Today,
    
    /// Show usage statistics for a specific date (YYYY-MM-DD)
    #[command(about = "Display usage statistics for a specific date")]
    Date { 
        /// Date to query in YYYY-MM-DD format (e.g., 2026-05-24)
        date: String 
    },
    
    /// Show usage statistics for a date range
    #[command(about = "Display usage statistics spanning multiple days")]
    Range { 
        /// Start date in YYYY-MM-DD format
        from: String, 
        /// End date in YYYY-MM-DD format
        to: String 
    },
}

#[derive(Serialize)]
struct Request {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Response {
    ok: bool,
    data: Option<serde_json::Value>,
    error: Option<String>,
}

fn get_socket_path() -> Result<PathBuf> {
    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .context("XDG_RUNTIME_DIR environment variable not set")?;
    Ok(PathBuf::from(runtime_dir).join("track-me.sock"))
}

fn send_request(req: Request) -> Result<serde_json::Value> {
    let socket_path = get_socket_path()?;
    
    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("Failed to connect to the track-me engine at {}. Is the background service running?", socket_path.display()))?;
        
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    
    let mut req_str = serde_json::to_string(&req)?;
    req_str.push('\n');
    
    stream.write_all(req_str.as_bytes())?;
    stream.flush()?;
    
    let mut reader = BufReader::new(stream);
    let mut response_str = String::new();
    reader.read_line(&mut response_str)?;
    
    let response: Response = serde_json::from_str(&response_str)
        .context("Failed to parse response from engine")?;
        
    if response.ok {
        Ok(response.data.unwrap_or(serde_json::Value::Null))
    } else {
        anyhow::bail!("Engine error: {}", response.error.unwrap_or_else(|| "Unknown error".into()))
    }
}

fn format_duration(ms: i64) -> String {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

fn print_stats_table(title: &str, data: HashMap<String, i64>) {
    if data.is_empty() {
        println!("No data available for {}", title);
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Application").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Time Spent").add_attribute(Attribute::Bold).fg(Color::Green),
        ]);

    // Sort by duration descending
    let mut entries: Vec<_> = data.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    let mut total_ms = 0;
    for (app, ms) in entries {
        total_ms += ms;
        table.add_row(vec![
            Cell::new(app),
            Cell::new(format_duration(ms)),
        ]);
    }

    println!("\n=== {} ===", title);
    println!("{table}");
    println!("Total Active Time: {}\n", format_duration(total_ms));
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            let data = send_request(Request {
                command: "current".into(),
                date: None,
                from: None,
                to: None,
            })?;
            
            println!("Engine Status:");
            let is_idle = data.get("is_idle").and_then(|v| v.as_bool()).unwrap_or(false);
            
            if is_idle {
                println!("  State: Idle (User is away)");
            } else {
                println!("  State: Active");
            }
            
            if let Some(class) = data.get("class").and_then(|v| v.as_str()) {
                let title = data.get("title").and_then(|v| v.as_str()).unwrap_or("Unknown");
                let duration_ms = data.get("focus_duration_ms").and_then(|v| v.as_i64()).unwrap_or(0);
                
                println!("  Focused App: {}", class);
                println!("  Window Title: {}", title);
                println!("  Current Focus Duration: {}", format_duration(duration_ms));
            } else {
                println!("  Focused App: None / Desktop");
            }
        }
        Commands::Today => {
            let data = send_request(Request {
                command: "today".into(),
                date: None,
                from: None,
                to: None,
            })?;
            
            let map: HashMap<String, i64> = serde_json::from_value(data)?;
            print_stats_table("Today's Usage", map);
        }
        Commands::Date { date } => {
            let data = send_request(Request {
                command: "date".into(),
                date: Some(date.clone()),
                from: None,
                to: None,
            })?;
            
            let map: HashMap<String, i64> = serde_json::from_value(data)?;
            print_stats_table(&format!("Usage on {}", date), map);
        }
        Commands::Range { from, to } => {
            let data = send_request(Request {
                command: "range".into(),
                date: None,
                from: Some(from.clone()),
                to: Some(to.clone()),
            })?;
            
            let map: HashMap<String, i64> = serde_json::from_value(data)?;
            print_stats_table(&format!("Usage from {} to {}", from, to), map);
        }
    }

    Ok(())
}
