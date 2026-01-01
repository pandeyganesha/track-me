use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Instant;
use regex::Regex;



fn main() -> std::io::Result<()> {

    const UNKNOWN_STATE: &str = "Unknown";

    let mut time_spent: HashMap<String, i32> = HashMap::from([(UNKNOWN_STATE.to_string(), 0)]);

    // Build the path to Hyprland's socket
    let instance = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .expect("HYPRLAND_INSTANCE_SIGNATURE not set");
    let mut socket_path = PathBuf::from(env::var("XDG_RUNTIME_DIR").unwrap());
    socket_path.push(format!("hypr/{}/.socket2.sock", instance));

    println!("Connecting to socket: {:?}", socket_path);

    // Connect to the Unix socket
    let stream = UnixStream::connect(socket_path)?;
    let reader = BufReader::new(stream);

    let re = Regex::new(r"^activewindow>>([^,]*),(.*)$").unwrap();
    let mut curr_active_win = UNKNOWN_STATE.to_string();
    let mut start = Instant::now();
    
    // Listen for events line by line
    for line in reader.lines() {


        let line = line?;
        if let Some(caps) = re.captures(&line) {
            let elapsed = start.elapsed().as_millis() as i32;
            start = Instant::now();
            
            // Update hashmap with the value
            // if the tool already in hashmap, then update the time by adding
            // if not then add new key with the time
            time_spent
            .entry(curr_active_win.to_string())
            .and_modify(|v| *v += elapsed)
            .or_insert(elapsed);
            println!("{:?}", time_spent);
            curr_active_win = caps[1].to_string();
            if curr_active_win.is_empty() {
                curr_active_win = "desktop".to_string();
            }
            // let context = caps[2].to_string();

        }
    }

    Ok(())
}
