use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use regex::Regex;

const UNKNOWN_STATE: &str = "Unknown";
const BROADCAST_SOCKET: &str = "/tmp/hyprland_time_tracker.sock";

type TimeSpent = Arc<Mutex<HashMap<String, i32>>>;

fn main() -> std::io::Result<()> {
    let time_spent: TimeSpent = Arc::new(Mutex::new(HashMap::from([
        (UNKNOWN_STATE.to_string(), 0)
    ])));

    // Start the broadcast server in a separate thread
    let time_spent_clone = Arc::clone(&time_spent);
    thread::spawn(move || {
        if let Err(e) = start_broadcast_server(time_spent_clone) {
            eprintln!("Broadcast server error: {}", e);
        }
    });

    // Connect to Hyprland socket and track window time
    track_window_time(time_spent)?;

    Ok(())
}

/// Connects to Hyprland's event socket and tracks time spent in each window
fn track_window_time(time_spent: TimeSpent) -> std::io::Result<()> {
    let socket_path = get_hyprland_socket_path()?;
    println!("Connecting to socket: {:?}", socket_path);

    let stream = UnixStream::connect(socket_path)?;
    let reader = BufReader::new(stream);
    let re = Regex::new(r"^activewindow>>([^,]*),(.*)$").unwrap();

    let mut curr_active_win = UNKNOWN_STATE.to_string();
    let mut start = Instant::now();

    for line in reader.lines() {
        let line = line?;
        println!("{}", line);

        if let Some(caps) = re.captures(&line) {
            let elapsed = start.elapsed().as_millis() as i32;
            start = Instant::now();

            // Update time spent for the previous window
            update_time_spent(&time_spent, &curr_active_win, elapsed);

            // Get new active window
            curr_active_win = caps[1].to_string();
            if curr_active_win.is_empty() {
                curr_active_win = "desktop".to_string();
            }
        }
    }

    Ok(())
}

/// Updates the time spent HashMap with the elapsed time for a window
fn update_time_spent(time_spent: &TimeSpent, window: &str, elapsed: i32) {
    let mut map = time_spent.lock().unwrap();
    map.entry(window.to_string())
        .and_modify(|v| *v += elapsed)
        .or_insert(elapsed);
}

/// Builds the path to Hyprland's event socket
fn get_hyprland_socket_path() -> std::io::Result<PathBuf> {
    let instance = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HYPRLAND_INSTANCE_SIGNATURE not set"
        ))?;

    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .map_err(|_| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "XDG_RUNTIME_DIR not set"
        ))?;

    let mut socket_path = PathBuf::from(runtime_dir);
    socket_path.push(format!("hypr/{}/.socket2.sock", instance));

    Ok(socket_path)
}

/// Starts a Unix socket server that broadcasts time spent data to clients
fn start_broadcast_server(time_spent: TimeSpent) -> std::io::Result<()> {
    // Remove existing socket file if it exists
    let _ = std::fs::remove_file(BROADCAST_SOCKET);

    let listener = UnixListener::bind(BROADCAST_SOCKET)?;
    println!("Broadcast server listening on: {}", BROADCAST_SOCKET);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let time_spent_clone = Arc::clone(&time_spent);
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream, time_spent_clone) {
                        eprintln!("Client handler error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("Connection failed: {}", e),
        }
    }

    Ok(())
}

/// Handles a client connection and sends the time spent report
fn handle_client(mut stream: UnixStream, time_spent: TimeSpent) -> std::io::Result<()> {
    let report = generate_report(&time_spent);
    stream.write_all(report.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(())
}

/// Generates a human-readable report of time spent per program
fn generate_report(time_spent: &TimeSpent) -> String {
    let map = time_spent.lock().unwrap();
    
    let mut report = String::from("=== Time Spent Report ===\n");
    
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1)); // Sort by time descending
    
    for (program, time_ms) in entries {
        let seconds = time_ms / 1000;
        let minutes = seconds / 60;
        let hours = minutes / 60;
        
        let time_str = if hours > 0 {
            format!("{}h {}m {}s", hours, minutes % 60, seconds % 60)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds % 60)
        } else {
            format!("{}s", seconds)
        };
        
        report.push_str(&format!("{}: {}\n", program, time_str));
    }
    
    report
}