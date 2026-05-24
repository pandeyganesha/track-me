# track-me

A local, privacy-first system activity tracker for Linux. 

It records your high-level computer activity as immutable, append-only events (window focus changes, idle states) and provides a fast SQLite index for querying your usage statistics over time.

## Architecture

The engine runs as a background daemon and consists of:
1. **Event Provider:** Connects to your compositor (currently Hyprland) to listen for window events.
2. **Idle Detector:** Tracks inactivity and pauses tracking when you step away.
3. **Tracker:** Maintains the state machine and calculates accurate time spans.
4. **Storage (Dual Write):** 
   - **JSONL Event Log (`~/.local/share/track-me/events/`):** The immutable source of truth. Every event is appended here.
   - **SQLite Index (`~/.local/share/track-me/index.db`):** An index built from the events to allow fast time-range queries.
5. **IPC Server:** A Unix socket (`$XDG_RUNTIME_DIR/track-me.sock`) that accepts JSON requests and returns JSON responses.

## Installation & Setup

1. **Build the binary:**
   ```bash
   cargo build --release
   ```

2. **Install the systemd service:**
   We provide a systemd user service to run the engine automatically in the background.
   ```bash
   mkdir -p ~/.config/systemd/user/
   cp ../track-me.service ~/.config/systemd/user/
   
   # Reload systemd and enable/start the service
   systemctl --user daemon-reload
   systemctl --user enable --now track-me.service
   ```

3. **Check the logs:**
   ```bash
   journalctl --user -u track-me -f
   ```

## Configuration

Configuration is optional. The engine will auto-detect your Hyprland session out of the box. 
If you want to customize it, create `~/.config/track-me/config.toml`:

```toml
[general]
# "auto" (default) or "hyprland"
provider = "auto"

[idle]
enabled = true
timeout_secs = 300 # 5 minutes

[storage]
# data_dir = "/custom/path/to/data"
```

## Querying Data (IPC)

The engine exposes a JSON-RPC-like interface over a Unix socket at `$XDG_RUNTIME_DIR/track-me.sock`.

You can query it using `socat`.

**Get today's usage:**
```bash
echo '{"command": "today"}' | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/track-me.sock
```

**Get current focus state:**
```bash
echo '{"command": "current"}' | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/track-me.sock
```

**Get usage for a specific date:**
```bash
echo '{"command": "date", "date": "2026-05-24"}' | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/track-me.sock
```

**Get usage for a date range:**
```bash
echo '{"command": "range", "from": "2026-05-01", "to": "2026-05-31"}' | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/track-me.sock
```
