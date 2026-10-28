//! Bring-your-own OpenRouter key. The key stays in the OS keychain and is only
//! sent to openrouter.ai.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const BASE: &str = "https://openrouter.ai/api/v1";

#[derive(Serialize, Clone, Debug)]
pub struct SttModel {
    pub id: String,
    pub name: String,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(70))
        .user_agent(concat!("Zpeak/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("http client")
}

/// Models whose output modality is `transcription` — the ones
/// `/audio/transcriptions` accepts. Public endpoint, no key needed.
pub async fn list_models() -> Result<Vec<SttModel>, String> {
    #[derive(Deserialize)]
    struct Model {
        id: String,
        name: String,
    }
    #[derive(Deserialize)]
    struct List {
        data: Vec<Model>,
    }
    let res = client()
        .get(format!("{BASE}/models?output_modalities=transcription"))
        .send()
        .await
        .map_err(|_| "NetworkError".to_string())?;
    if !res.status().is_success() {
        return Err("ModelListFailed".into());
    }
    let list: List = res.json().await.map_err(|_| "ModelListFailed".to_string())?;
    let mut models: Vec<SttModel> = list
        .data
        .into_iter()
        .map(|m| SttModel { id: m.id, name: m.name })
        .collect();
    models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(models)
}

/// Confirms the key works before we store it.
pub async fn verify_key(key: &str) -> Result<(), String> {
    let res = client()
        .get(format!("{BASE}/key"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| "NetworkError".to_string())?;
    if res.status().is_success() {
        Ok(())
    } else {
        Err("InvalidApiKey".into())
    }
}

pub async fn transcribe(
    key: &str,
    model: &str,
    wav: &[u8],
    language: Option<&str>,
) -> Result<String, String> {
    let mut body = serde_json::json!({
        "model": model,
        "input_audio": { "data": STANDARD.encode(wav), "format": "wav" },
    });
    if let Some(lang) = language {
        // OpenRouter expects ISO 639-1; Cantonese goes as Chinese.
        body["language"] = serde_json::json!(if lang == "yue" { "zh" } else { lang });
    }
    let res = client()
        .post(format!("{BASE}/audio/transcriptions"))
        .bearer_auth(key)
        .header("HTTP-Referer", "https://github.com/Zense-AI/zpeak")
        .header("X-Title", "Zpeak")
        .json(&body)
        .send()
        .await
        .map_err(|_| "NetworkError".to_string())?;
    match res.status().as_u16() {
        200 => {}
        401 | 403 => return Err("InvalidApiKey".into()),
        402 => return Err("OutOfCredits".into()),
        429 => return Err("RateLimited".into()),
        _ => return Err("TranscriptionFailed".into()),
    }
    #[derive(Deserialize)]
    struct Out {
        text: Option<String>,
    }
    let out: Out = res.json().await.map_err(|_| "TranscriptionFailed".to_string())?;
    Ok(out.text.unwrap_or_default())
}
