//! App settings (recent folders, active provider) in the app config dir, and API keys in the OS keychain.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const MAX_RECENT_FOLDERS: usize = 10;
const KEYCHAIN_SERVICE: &str = "ai-ide";

/// Environment variables that hold provider keys. Each is also stored in the keychain under the same name.
pub const KEY_NAMES: [&str; 2] = ["ANTHROPIC_API_KEY", "GEMINI_API_KEY"];

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent_folders: Vec<String>,
    pub active_provider: Option<String>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// Missing or unreadable settings fall back to defaults; they must never stop the app from starting.
pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Moves `root` to the front of the recent list.
pub fn remember_folder(app: &AppHandle, root: &Path) -> Result<(), String> {
    let mut settings = load(app);
    let entry = root.display().to_string();
    settings.recent_folders.retain(|f| f != &entry);
    settings.recent_folders.insert(0, entry);
    settings.recent_folders.truncate(MAX_RECENT_FOLDERS);
    save(app, &settings)
}

pub fn remember_provider(app: &AppHandle, id: &str) -> Result<(), String> {
    let mut settings = load(app);
    settings.active_provider = Some(id.to_string());
    save(app, &settings)
}

/// Copies keys from the keychain into the environment, where the server reads them.
/// Keys already set in the environment win, so a developer's shell settings still apply.
pub fn load_keys_into_env() {
    for name in KEY_NAMES {
        if std::env::var(name).is_ok() {
            continue;
        }
        if let Ok(value) = keyring::Entry::new(KEYCHAIN_SERVICE, name).and_then(|e| e.get_password()) {
            std::env::set_var(name, value);
        }
    }
}

pub fn store_key(name: &str, value: &str) -> Result<(), String> {
    if !KEY_NAMES.contains(&name) {
        return Err(format!("'{}' is not a provider key", name));
    }
    keyring::Entry::new(KEYCHAIN_SERVICE, name)
        .and_then(|e| e.set_password(value))
        .map_err(|e| format!("Could not save {} to the keychain: {}", name, e))?;
    std::env::set_var(name, value);
    Ok(())
}

pub fn key_present(name: &str) -> bool {
    std::env::var(name).map(|v| !v.trim().is_empty()).unwrap_or(false)
}
