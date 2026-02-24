use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::str::FromStr;
use tauri_plugin_global_shortcut::{Modifiers, Shortcut};

pub const DEFAULT_HOTKEY: &str = "control+shift+KeyS";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub hotkey: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
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

    let settings = AppSettings { hotkey: canonical };
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
