//! OS keychain storage (macOS Keychain, Windows Credential Manager).
//! Nothing secret is ever written to disk in plain text or sent to the webview.

use keyring::Entry;

const SERVICE: &str = "io.zenseai.zpeak";
pub const ZENSE_TOKENS: &str = "zense-tokens";
pub const OPENROUTER_KEY: &str = "openrouter-api-key";

fn entry(name: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, name).map_err(|e| e.to_string())
}

pub fn get(name: &str) -> Option<String> {
    entry(name).ok()?.get_password().ok()
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    entry(name)?.set_password(value).map_err(|e| e.to_string())
}

pub fn delete(name: &str) {
    if let Ok(e) = entry(name) {
        let _ = e.delete_credential();
    }
}
