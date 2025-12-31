use std::env;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;


fn main() -> std::io::Result<()> {
    // Build the path to Hyprland's socket
    let instance = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .expect("HYPRLAND_INSTANCE_SIGNATURE not set");
    let mut socket_path = PathBuf::from(env::var("XDG_RUNTIME_DIR").unwrap());
    socket_path.push(format!("hypr/{}/.socket2.sock", instance));

    println!("Connecting to socket: {:?}", socket_path);

    // Connect to the Unix socket
    let stream = UnixStream::connect(socket_path)?;
    let reader = BufReader::new(stream);

    // Listen for events line by line
    for line in reader.lines() {
        let line = line?;
        println!("Active window event: {}", line);
    }

    Ok(())
}
