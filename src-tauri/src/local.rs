//! Offline speech-to-text with whisper.cpp (MIT). Models are downloaded from
//! Hugging Face on demand and verified against pinned SHA-256 hashes.

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct ModelInfo {
    pub id: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
    /// Supports the dedicated Cantonese (`yue`) language token.
    pub cantonese: bool,
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "tiny",
        file: "ggml-tiny.bin",
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        bytes: 77_691_713,
        cantonese: false,
    },
    ModelInfo {
        id: "base",
        file: "ggml-base.bin",
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        bytes: 147_951_465,
        cantonese: false,
    },
    ModelInfo {
        id: "small",
        file: "ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        bytes: 487_601_967,
        cantonese: false,
    },
    ModelInfo {
        id: "large-v3-turbo",
        file: "ggml-large-v3-turbo-q5_0.bin",
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        bytes: 574_041_195,
        cantonese: true,
    },
];

pub fn model(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: &'static str,
    pub bytes: u64,
    pub downloaded: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    received: u64,
    total: u64,
}

#[derive(Default)]
pub struct LocalEngine {
    loaded: Mutex<Option<(String, WhisperContext)>>,
    downloading: Mutex<Option<String>>,
    cancel: AtomicBool,
}

fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("models");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn model_path(app: &AppHandle, m: &ModelInfo) -> Result<PathBuf, String> {
    Ok(models_dir(app)?.join(m.file))
}

pub fn statuses(app: &AppHandle) -> Vec<ModelStatus> {
    MODELS
        .iter()
        .map(|m| ModelStatus {
            id: m.id,
            bytes: m.bytes,
            downloaded: model_path(app, m)
                .ok()
                .and_then(|p| std::fs::metadata(p).ok())
                .is_some_and(|meta| meta.len() == m.bytes),
        })
        .collect()
}

impl LocalEngine {
    pub fn downloading(&self) -> Option<String> {
        self.downloading.lock().unwrap().clone()
    }

    pub fn cancel_download(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub async fn download(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let m = model(id).ok_or("UnknownModel")?;
        {
            let mut d = self.downloading.lock().unwrap();
            if d.is_some() {
                return Err("DownloadInProgress".into());
            }
            *d = Some(id.to_string());
        }
        self.cancel.store(false, Ordering::SeqCst);
        let result = self.download_inner(app, m).await;
        *self.downloading.lock().unwrap() = None;
        result
    }

    async fn download_inner(&self, app: &AppHandle, m: &ModelInfo) -> Result<(), String> {
        let dest = model_path(app, m)?;
        let part = dest.with_extension("part");
        let url = format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            m.file
        );
        let res = reqwest::Client::builder()
            .user_agent(concat!("Zpeak/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?
            .get(url)
            .send()
            .await
            .map_err(|_| "NetworkError".to_string())?;
        if !res.status().is_success() {
            return Err("DownloadFailed".into());
        }

        let mut file = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        let mut last_emit = 0u64;
        let mut stream = res.bytes_stream();
        let outcome: Result<(), String> = async {
            while let Some(chunk) = stream.next().await {
                if self.cancel.load(Ordering::SeqCst) {
                    return Err("DownloadCancelled".into());
                }
                let chunk = chunk.map_err(|_| "NetworkError".to_string())?;
                received += chunk.len() as u64;
                if received > m.bytes {
                    return Err("DownloadCorrupt".into());
                }
                hasher.update(&chunk);
                tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                    .await
                    .map_err(|_| "DiskFull".to_string())?;
                if received - last_emit > 2_000_000 || received == m.bytes {
                    last_emit = received;
                    let _ = app.emit(
                        "model-download",
                        Progress { id: m.id.into(), received, total: m.bytes },
                    );
                }
            }
            tokio::io::AsyncWriteExt::flush(&mut file)
                .await
                .map_err(|_| "DiskFull".to_string())?;
            let digest = format!("{:x}", hasher.finalize());
            if received != m.bytes || digest != m.sha256 {
                return Err("DownloadCorrupt".into());
            }
            Ok(())
        }
        .await;
        drop(file);

        if let Err(e) = outcome {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(e);
        }
        tokio::fs::rename(&part, &dest).await.map_err(|e| e.to_string())
    }

    pub fn delete(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let m = model(id).ok_or("UnknownModel")?;
        let mut loaded = self.loaded.lock().unwrap();
        if loaded.as_ref().is_some_and(|(loaded_id, _)| loaded_id == id) {
            *loaded = None;
        }
        let p = model_path(app, m)?;
        if p.exists() {
            std::fs::remove_file(p).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Blocking: call from `spawn_blocking`.
    pub fn transcribe(
        &self,
        app: &AppHandle,
        id: &str,
        samples: &[f32],
        language: Option<&str>,
        script_hint: Option<&str>,
    ) -> Result<String, String> {
        let m = model(id).ok_or("UnknownModel")?;
        let path = model_path(app, m)?;
        if !statuses(app).iter().any(|s| s.id == id && s.downloaded) {
            return Err("ModelNotDownloaded".into());
        }

        let mut loaded = self.loaded.lock().unwrap();
        if loaded.as_ref().is_none_or(|(loaded_id, _)| loaded_id != id) {
            *loaded = None;
            let ctx = WhisperContext::new_with_params(
                path.to_str().ok_or("BadModelPath")?,
                WhisperContextParameters::default(),
            )
            .map_err(|_| "ModelLoadFailed".to_string())?;
            *loaded = Some((id.to_string(), ctx));
        }
        let (_, ctx) = loaded.as_ref().unwrap();
        let mut state = ctx.create_state().map_err(|_| "ModelLoadFailed".to_string())?;

        let lang = match language {
            Some("yue") if !m.cantonese => Some("zh"),
            other => other,
        };
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(lang.unwrap_or("auto")));
        params.set_n_threads(
            std::thread::available_parallelism()
                .map(|n| n.get().min(8) as i32)
                .unwrap_or(4),
        );
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_no_context(true);
        if let Some(prompt) = script_hint {
            params.set_initial_prompt(prompt);
        }

        state
            .full(params, samples)
            .map_err(|_| "TranscriptionFailed".to_string())?;
        let n = state.full_n_segments().map_err(|_| "TranscriptionFailed".to_string())?;
        let mut text = String::new();
        for i in 0..n {
            if let Ok(seg) = state.full_get_segment_text(i) {
                text.push_str(&seg);
            }
        }
        Ok(clean(&text))
    }
}

/// Drops whisper's non-speech markers such as `[BLANK_AUDIO]` or `(music)`.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_hashes_look_right() {
        for (i, m) in MODELS.iter().enumerate() {
            assert_eq!(m.sha256.len(), 64);
            assert!(MODELS[i + 1..].iter().all(|o| o.id != m.id));
        }
    }

    #[test]
    fn clean_removes_markers() {
        assert_eq!(clean(" [BLANK_AUDIO] "), "");
        assert_eq!(clean(" Hello (music) world "), "Hello world");
        assert_eq!(clean("你好 世界"), "你好 世界");
    }
}
