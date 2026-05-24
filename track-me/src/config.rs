// track-me: Configuration management
//
// Loads settings from `~/.config/track-me/config.toml` with sensible
// defaults. The config file is optional — the engine works out of the
// box with auto-detected settings.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Top-level configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub idle: IdleConfig,
    pub storage: StorageConfig,
}

/// General engine settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Which compositor provider to use.
    /// "auto" detects from environment variables.
    /// Explicit values: "hyprland", "sway", "x11"
    pub provider: String,
}

/// Idle detection settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IdleConfig {
    /// Whether idle detection is enabled.
    pub enabled: bool,
    /// Seconds of inactivity before marking as idle.
    pub timeout_secs: u64,
}

/// Storage settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Override for the data directory.
    /// Default: `$XDG_DATA_HOME/track-me` (typically `~/.local/share/track-me`)
    pub data_dir: Option<PathBuf>,
}

// --- Defaults ---

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            idle: IdleConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            provider: "auto".to_string(),
        }
    }
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_secs: 300, // 5 minutes
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { data_dir: None }
    }
}

impl Config {
    /// Load configuration from the standard XDG config path.
    ///
    /// If the config file doesn't exist, returns defaults.
    /// If it exists but is malformed, returns an error.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config: {}", config_path.display()))?;
            let config: Config = toml::from_str(&contents)
                .with_context(|| format!("Failed to parse config: {}", config_path.display()))?;
            Ok(config)
        } else {
            log::info!(
                "No config file at {}, using defaults",
                config_path.display()
            );
            Ok(Config::default())
        }
    }

    /// Returns the resolved data directory path.
    pub fn data_dir(&self) -> PathBuf {
        self.storage
            .data_dir
            .clone()
            .unwrap_or_else(|| {
                dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("~/.local/share"))
                    .join("track-me")
            })
    }

    /// Returns the path to the IPC socket.
    pub fn ipc_socket_path(&self) -> PathBuf {
        // Use XDG_RUNTIME_DIR for the socket (per-session, tmpfs, secure)
        if let Some(runtime_dir) = dirs::runtime_dir() {
            runtime_dir.join("track-me.sock")
        } else {
            // Fallback (shouldn't happen on systemd-based systems)
            PathBuf::from("/tmp/track-me.sock")
        }
    }

    /// Standard config file path: `~/.config/track-me/config.toml`
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("track-me")
            .join("config.toml")
    }
}
