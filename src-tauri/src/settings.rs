//! Non-secret preferences, stored as JSON in the app config dir.
//! Secrets (tokens, API keys) live in the OS keychain — see `secrets.rs`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Zense,
    Local,
    OpenRouter,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecordMode {
    /// Press once to start, again to stop.
    Toggle,
    /// Record while the shortcut is held down.
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub provider: Option<Provider>,
    pub shortcut: String,
    pub mode: RecordMode,
    /// `auto` or an ISO 639-1/3 code (`en`, `zh`, `yue`, ...).
    pub language: String,
    /// `en`, `zh-hant`, `zh-hans`; `None` follows the OS.
    pub ui_language: Option<String>,
    pub local_model: String,
    pub openrouter_model: Option<String>,
    pub restore_clipboard: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: None,
            shortcut: "Alt+Z".into(),
            mode: RecordMode::Toggle,
            language: "auto".into(),
            ui_language: None,
            local_model: "base".into(),
            openrouter_model: None,
            restore_clipboard: true,
        }
    }
}

const LANGUAGES: &[&str] = &["auto", "en", "zh", "yue", "ja", "ko", "fr", "de", "es"];
const UI_LANGUAGES: &[&str] = &["en", "zh-hant", "zh-hans"];

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !LANGUAGES.contains(&self.language.as_str()) {
            return Err("UnsupportedLanguage".into());
        }
        if let Some(ui) = &self.ui_language {
            if !UI_LANGUAGES.contains(&ui.as_str()) {
                return Err("UnsupportedUiLanguage".into());
            }
        }
        if crate::local::model(&self.local_model).is_none() {
            return Err("UnknownModel".into());
        }
        if let Some(model) = &self.openrouter_model {
            if model.len() > 200 || model.chars().any(|c| c.is_whitespace()) {
                return Err("InvalidModel".into());
            }
        }
        Ok(())
    }
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
        .filter(|s| s.validate().is_ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let p = path(app)?;
    let tmp = p.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}
