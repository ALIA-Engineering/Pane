//! Pane configuration - persisted to disk as JSON.
//!
//! Saved to: ~/.config/pane/config.json (Linux/Mac)
//!           %APPDATA%/pane/config.json (Windows)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_theme")]
    pub theme: String,  // "dark", "light", "system"

    #[serde(default = "default_refresh_ms")]
    pub refresh_ms: u64,

    #[serde(default)]
    pub selected_gpu: usize,

    #[serde(default = "default_window_width")]
    pub window_width: f32,

    #[serde(default = "default_window_height")]
    pub window_height: f32,

    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,

    #[serde(default = "default_panel")]
    pub default_panel: String,

    // Adapter-name substring for the GPU the app renders on. None = automatic.
    // Applied at startup; overridden by the --gpu CLI flag.
    #[serde(default)]
    pub render_gpu: Option<String>,
}

fn default_theme() -> String { "dark".into() }
fn default_refresh_ms() -> u64 { 500 }
fn default_window_width() -> f32 { 1100.0 }
fn default_window_height() -> f32 { 700.0 }
fn default_sidebar_width() -> f32 { 170.0 }
fn default_panel() -> String { "dashboard".into() }

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            refresh_ms: default_refresh_ms(),
            selected_gpu: 0,
            window_width: default_window_width(),
            window_height: default_window_height(),
            sidebar_width: default_sidebar_width(),
            default_panel: default_panel(),
            render_gpu: None,
        }
    }
}

impl Config {
    /// Get the config file path.
    fn path() -> Option<PathBuf> {
        let dir = dirs::config_dir()?.join("pane");
        Some(dir.join("config.json"))
    }

    /// Load config from disk, or return defaults if not found.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };

        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save config to disk.
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Convert theme string to ThemeMode.
    pub fn theme_mode(&self) -> crate::gui::theme::ThemeMode {
        match self.theme.as_str() {
            "light" => crate::gui::theme::ThemeMode::Light,
            "system" => crate::gui::theme::ThemeMode::System,
            _ => crate::gui::theme::ThemeMode::Dark,
        }
    }
}
