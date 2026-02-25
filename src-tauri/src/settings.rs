use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::str::FromStr;
use tauri_plugin_global_shortcut::{Modifiers, Shortcut};

pub const DEFAULT_HOTKEY: &str = "control+shift+KeyS";
pub const DEFAULT_AUTO_COPY: bool = false;

fn default_hotkey() -> String {
    DEFAULT_HOTKEY.to_string()
}

fn default_auto_copy() -> bool {
    DEFAULT_AUTO_COPY
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_auto_copy")]
    pub auto_copy: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
            auto_copy: DEFAULT_AUTO_COPY,
        }
    }
}

pub fn load(path: &Path) -> AppSettings {
    let mut settings = match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<AppSettings>(&raw).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    };

    settings.hotkey =
        canonicalize_hotkey(&settings.hotkey).unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
    settings
}

pub fn save_hotkey(path: &Path, hotkey: &str) -> std::result::Result<(), String> {
    let canonical =
        canonicalize_hotkey(hotkey).ok_or_else(|| "Invalid hotkey format".to_string())?;

    let mut settings = load(path);
    settings.hotkey = canonical;
    write(path, &settings)
}

pub fn save_auto_copy(path: &Path, enabled: bool) -> std::result::Result<(), String> {
    let mut settings = load(path);
    settings.auto_copy = enabled;
    write(path, &settings)
}

fn write(path: &Path, settings: &AppSettings) -> std::result::Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create settings directory: {e}"))?;
    }

    let data = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    fs::write(path, data).map_err(|e| format!("Failed to write settings: {e}"))?;
    Ok(())
}

pub fn canonicalize_hotkey(raw: &str) -> Option<String> {
    let shortcut = Shortcut::from_str(raw.trim()).ok()?;
    let required_mods = Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER;
    if !shortcut.mods.intersects(required_mods) {
        return None;
    }
    Some(shortcut.to_string())
}
