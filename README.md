# track-me

**track-me** is a local, privacy-first system activity tracker for Linux. 

It silently runs in the background to record your high-level computer activity (such as window focus changes and idle time) as immutable, append-only events. It provides accurate time-tracking statistics without ever sending your data to the cloud.

---

## 🧠 How It Works

This project is built using the Unix philosophy: splitting complex tasks into small, focused tools that communicate with each other. It consists of two main components:

### 1. The Core Engine (`track-me`)
A background daemon that runs continuously. You never interact with this directly.
* **Event Provider:** Hooks directly into your compositor (currently Hyprland) to listen for window focus events exactly when they happen.
* **Idle Detector:** Notices if you haven't typed or moved your mouse for a while (configurable) and automatically pauses the timer so your "time spent" remains accurate.
* **Dual Storage System:**
  * **JSONL Event Log (`~/.local/share/track-me/events/`):** The immutable source of truth. Every single event is permanently logged here.
  * **SQLite Index (`~/.local/share/track-me/index.db`):** A fast database built dynamically from the events, enabling lightning-fast time aggregations.
* **IPC Server:** It opens a Unix Domain Socket at `$XDG_RUNTIME_DIR/track-me.sock` that accepts JSON requests.

### 2. The CLI Dashboard (`track-me-cli`)
The command-line tool you use to see your data. It connects to the Core Engine's Unix socket, asks for statistics, and prints beautifully formatted tables right in your terminal.

---

## 🛠️ Installation & Setup

1. **Build the Project**
   Ensure you have Rust installed, then compile the project:
   ```bash
   git clone <your-repo>
   cd track-me/track-me
   cargo install --path .
   ```
   *This installs two binaries into `~/.cargo/bin`: `track-me` and `track-me-cli`.*

2. **Install the Background Service**
   We provide a Systemd user service so the engine starts automatically when you log in.
   ```bash
   mkdir -p ~/.config/systemd/user/
   cp track-me.service ~/.config/systemd/user/
   
   # Reload systemd and start the service
   systemctl --user daemon-reload
   systemctl --user enable --now track-me.service
   ```

3. **Check the Engine Logs**
   To verify the engine is running properly:
   ```bash
   journalctl --user -u track-me -f
   ```

---

## 📊 Usage

To view your statistics, use the `track-me-cli` command. 

**Check Live Status**  
See what the engine is tracking *right now*, and whether you are currently marked as Active or Idle.
```bash
track-me-cli status
```

**View Today's Summary**  
See a table of all applications used today and the total time spent on each.
```bash
track-me-cli today
```

**View a Specific Date**  
```bash
track-me-cli date 2026-05-24
```

**View a Date Range**  
```bash
track-me-cli range 2026-05-01 2026-05-31
```

---

## ⚙️ Configuration

Configuration is entirely optional. Out of the box, `track-me` will auto-detect your Hyprland session and start tracking.

To customize behavior, create a config file at `~/.config/track-me/config.toml`:

```toml
[general]
# "auto" (default) detects your environment. Explicit: "hyprland"
provider = "auto"

[idle]
enabled = true
# How many seconds of inactivity before marking you as "Idle"
timeout_secs = 300 

[storage]
# Override the default data directory if desired
# data_dir = "/custom/path/to/data"
```
*Note: If you change the config, restart the engine with `systemctl --user restart track-me.service`.*

---

## 💻 For Developers: Building Custom UIs

Because the Core Engine exposes its data over a local Unix Socket, you are not limited to the CLI! You can build Web Dashboards, Python scripts, or desktop widgets by querying the socket directly.

The socket is located at `$XDG_RUNTIME_DIR/track-me.sock`. Send a single line of JSON to it, and it will respond with JSON.

**Example using `socat` in bash:**
```bash
echo '{"command": "today"}' | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/track-me.sock
```

**Supported Commands:**
* `{"command": "status"}` - Returns current engine status.
* `{"command": "current"}` - Returns details about the currently focused window.
* `{"command": "today"}` - Returns a dictionary of `{ "app_name": duration_in_milliseconds }`.
* `{"command": "date", "date": "YYYY-MM-DD"}` - Returns usage for a specific day.
* `{"command": "range", "from": "YYYY-MM-DD", "to": "YYYY-MM-DD"}` - Returns usage spanning multiple days.

---

## 📂 Data Privacy & File Locations

Your data is entirely local.
* **Logs & Database:** `~/.local/share/track-me/`
* **Configuration:** `~/.config/track-me/config.toml`
* **Live Socket:** `$XDG_RUNTIME_DIR/track-me.sock` (usually `/run/user/1000/track-me.sock`)
