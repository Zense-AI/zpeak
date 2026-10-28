//! "Sign in with Zense AI": OAuth 2.0 authorization code + PKCE with a
//! loopback redirect (RFC 8252). Zpeak is a public client and ships no
//! secret, so publishing the source does not weaken the flow: a code is only
//! useful together with the verifier that never leaves this process.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::secrets;

pub const API_URL: &str = match option_env!("ZPEAK_API_URL") {
    Some(v) => v,
    None => "https://api.zenseai.io",
};
pub const APP_URL: &str = match option_env!("ZPEAK_APP_URL") {
    Some(v) => v,
    None => "https://app.zenseai.io",
};

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Serialize, Deserialize, Clone)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    /// Unix seconds.
    expires_at: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub username: String,
    pub email: String,
    pub eligible: bool,
}

/// Serialises token refreshes so two clips never rotate the same refresh token.
pub struct ZenseClient {
    http: reqwest::Client,
    lock: Mutex<()>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_b64(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn load_tokens() -> Option<Tokens> {
    serde_json::from_str(&secrets::get(secrets::ZENSE_TOKENS)?).ok()
}

fn store_tokens(t: &Tokens) -> Result<(), String> {
    secrets::set(
        secrets::ZENSE_TOKENS,
        &serde_json::to_string(t).map_err(|e| e.to_string())?,
    )
}

pub fn is_signed_in() -> bool {
    load_tokens().is_some()
}

const CALLBACK_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Zpeak</title>
<style>body{font-family:"Avenir Next",Avenir,"Noto Sans TC",system-ui,sans-serif;background:#F3F4F6;display:grid;place-items:center;height:100vh;margin:0;color:#0B0F14}
div{background:#fff;border-radius:16px;padding:32px 40px;text-align:center;max-width:360px}h1{font-weight:500;font-size:22px;margin:0 0 8px}p{color:#6B7280;margin:0}</style></head>
<body><div><h1>{title}</h1><p>{body}</p></div></body></html>"#;

fn page(title: &str, body: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let html = CALLBACK_PAGE.replace("{title}", title).replace("{body}", body);
    tiny_http::Response::from_data(html.into_bytes()).with_header(
        "Content-Type: text/html; charset=utf-8"
            .parse::<tiny_http::Header>()
            .unwrap(),
    )
}

/// Blocks (on a worker thread) until the browser hits the loopback callback.
fn wait_for_code(server: tiny_http::Server, state: String) -> Result<String, String> {
    let deadline = std::time::Instant::now() + SIGN_IN_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err("SignInTimedOut".into());
        }
        let Some(req) = server.recv_timeout(left).map_err(|e| e.to_string())? else {
            return Err("SignInTimedOut".into());
        };
        let url = match url::Url::parse(&format!("http://127.0.0.1{}", req.url())) {
            Ok(u) => u,
            Err(_) => {
                let _ = req.respond(tiny_http::Response::empty(400));
                continue;
            }
        };
        if url.path() != "/callback" {
            let _ = req.respond(tiny_http::Response::empty(404));
            continue;
        }
        let get = |k: &str| {
            url.query_pairs()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.into_owned())
        };
        if !get("state").is_some_and(|s| constant_time_eq(&s, &state)) {
            // Not our flow (stale tab or a forged request) — keep waiting.
            let _ = req.respond(tiny_http::Response::empty(400));
            continue;
        }
        if get("error").is_some() {
            let _ = req.respond(page("Zpeak", "Sign-in was cancelled. You can close this tab."));
            return Err("SignInCancelled".into());
        }
        let Some(code) = get("code").filter(|c| c.len() <= 128) else {
            let _ = req.respond(tiny_http::Response::empty(400));
            continue;
        };
        let _ = req.respond(page(
            "Zpeak is connected",
            "Go back to Zpeak. You can close this tab.",
        ));
        return Ok(code);
    }
}

impl ZenseClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(70))
                .user_agent(concat!("Zpeak/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("http client"),
            lock: Mutex::new(()),
        }
    }

    pub async fn sign_in(&self, open_url: impl FnOnce(&str) -> Result<(), String>) -> Result<Profile, String> {
        let verifier = random_b64(64);
        let state = random_b64(32);
        let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = server
            .server_addr()
            .to_ip()
            .map(|a| a.port())
            .ok_or("NoLoopbackPort")?;
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        let device = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "Zpeak".into());

        let mut authorize = url::Url::parse(&format!("{APP_URL}/oauth/zpeak")).map_err(|e| e.to_string())?;
        authorize
            .query_pairs_mut()
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge", &pkce_challenge(&verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("device_name", &device);
        open_url(authorize.as_str())?;

        let code = tokio::task::spawn_blocking(move || wait_for_code(server, state))
            .await
            .map_err(|e| e.to_string())??;

        let res = self
            .http
            .post(format!("{API_URL}/zpeak/oauth/token"))
            .json(&serde_json::json!({
                "grant_type": "authorization_code",
                "code": code,
                "code_verifier": verifier,
                "redirect_uri": redirect_uri,
            }))
            .send()
            .await
            .map_err(|_| "NetworkError".to_string())?;
        let tokens = Self::parse_tokens(res).await?;
        store_tokens(&tokens)?;
        self.profile().await
    }

    async fn parse_tokens(res: reqwest::Response) -> Result<Tokens, String> {
        if res.status() == reqwest::StatusCode::FORBIDDEN {
            return Err("ZpeakNotEligible".into());
        }
        if !res.status().is_success() {
            return Err("SignInFailed".into());
        }
        let t: TokenResponse = res.json().await.map_err(|_| "SignInFailed".to_string())?;
        Ok(Tokens {
            access_token: t.access_token,
            refresh_token: t.refresh_token,
            expires_at: now() + t.expires_in,
        })
    }

    /// Returns a live access token, rotating the refresh token when needed.
    async fn access_token(&self, force_refresh: bool) -> Result<String, String> {
        let _guard = self.lock.lock().await;
        let tokens = load_tokens().ok_or("SignedOut")?;
        if !force_refresh && tokens.expires_at > now() + 60 {
            return Ok(tokens.access_token);
        }
        let res = self
            .http
            .post(format!("{API_URL}/zpeak/oauth/token"))
            .json(&serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": tokens.refresh_token,
            }))
            .send()
            .await
            .map_err(|_| "NetworkError".to_string())?;
        match res.status().as_u16() {
            400 | 401 => {
                secrets::delete(secrets::ZENSE_TOKENS);
                return Err("SignedOut".into());
            }
            429 => return Err("RateLimited".into()),
            _ => {}
        }
        let fresh = Self::parse_tokens(res).await?;
        store_tokens(&fresh)?;
        Ok(fresh.access_token)
    }

    async fn authed(
        &self,
        build: impl Fn(&reqwest::Client, &str) -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, String> {
        let token = self.access_token(false).await?;
        let res = build(&self.http, &token)
            .send()
            .await
            .map_err(|_| "NetworkError".to_string())?;
        if res.status() != reqwest::StatusCode::UNAUTHORIZED {
            return Ok(res);
        }
        let token = self.access_token(true).await?;
        build(&self.http, &token)
            .send()
            .await
            .map_err(|_| "NetworkError".to_string())
    }

    pub async fn profile(&self) -> Result<Profile, String> {
        let res = self
            .authed(|http, token| http.get(format!("{API_URL}/zpeak/me")).bearer_auth(token))
            .await?;
        if !res.status().is_success() {
            return Err("ProfileFailed".into());
        }
        res.json().await.map_err(|_| "ProfileFailed".to_string())
    }

    pub async fn transcribe(&self, wav: &[u8], language: Option<&str>) -> Result<String, String> {
        let body = serde_json::json!({
            "file": STANDARD.encode(wav),
            "format": "wav",
            "languageCode": language,
        });
        let res = self
            .authed(|http, token| {
                http.post(format!("{API_URL}/zpeak/transcribe"))
                    .bearer_auth(token)
                    .json(&body)
            })
            .await?;
        match res.status().as_u16() {
            200 => {}
            403 => return Err("ZpeakNotEligible".into()),
            413 => return Err("AudioTooLong".into()),
            429 => return Err("RateLimited".into()),
            _ => return Err("TranscriptionFailed".into()),
        }
        #[derive(Deserialize)]
        struct Out {
            text: String,
        }
        let out: Out = res.json().await.map_err(|_| "TranscriptionFailed".to_string())?;
        Ok(out.text)
    }

    pub async fn sign_out(&self) {
        let _guard = self.lock.lock().await;
        if let Some(t) = load_tokens() {
            let _ = self
                .http
                .post(format!("{API_URL}/zpeak/oauth/revoke"))
                .json(&serde_json::json!({ "token": t.refresh_token }))
                .timeout(Duration::from_secs(10))
                .send()
                .await;
        }
        secrets::delete(secrets::ZENSE_TOKENS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn random_values_are_url_safe_and_long_enough() {
        let v = random_b64(64);
        assert!(v.len() >= 43 && v.len() <= 128);
        assert!(v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(random_b64(32), random_b64(32));
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }

    #[test]
    fn callback_rejects_wrong_state_then_accepts_right_one() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let handle = std::thread::spawn(move || wait_for_code(server, "good-state".into()));
        let get = |path: &str| {
            use std::io::{Read, Write};
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            out
        };
        assert!(get("/favicon.ico").starts_with("HTTP/1.1 404"));
        assert!(get("/callback?code=evil&state=bad").starts_with("HTTP/1.1 400"));
        assert!(get("/callback?code=zpk_c_1&state=good-state").starts_with("HTTP/1.1 200"));
        assert_eq!(handle.join().unwrap().unwrap(), "zpk_c_1");
    }
}
