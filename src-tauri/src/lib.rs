mod audio;
mod local;
mod openrouter;
mod paste;
mod secrets;
mod settings;
mod zense;

use serde::Serialize;
use settings::{Provider, RecordMode, Settings};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    Recording,
    Transcribing,
}

struct Core {
    settings: Mutex<Settings>,
    phase: Mutex<Phase>,
    recording: Mutex<Option<(audio::Recording, Instant)>>,
    zense: zense::ZenseClient,
    local: local::LocalEngine,
}

type Shared = Arc<Core>;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Hud {
    state: &'static str,
    code: Option<String>,
    level: u32,
    seconds: u32,
    lang: Option<String>,
}

fn ui_lang(app: &AppHandle) -> Option<String> {
    app.try_state::<Shared>()?.settings.lock().unwrap().ui_language.clone()
}

fn hud(app: &AppHandle, state: &'static str, code: Option<String>) {
    let lang = ui_lang(app);
    let _ = app.emit_to("overlay", "hud", Hud { state, code, level: 0, seconds: 0, lang });
}

fn set_tray_recording(app: &AppHandle, recording: bool) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let bytes: &[u8] = match (cfg!(target_os = "macos"), recording) {
        (true, false) => include_bytes!("../icons/tray/tray-template.png"),
        (true, true) => include_bytes!("../icons/tray/tray-template-recording.png"),
        (false, false) => include_bytes!("../icons/tray/tray.png"),
        (false, true) => include_bytes!("../icons/tray/tray-recording.png"),
    };
    if let Ok(img) = Image::from_bytes(bytes) {
        let _ = tray.set_icon(Some(img));
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_as_template(true);
    }
}

fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Everything needed to transcribe must be in place before we record, so a
/// student never talks for a minute into a dead end.
fn ready_error(core: &Core, app: &AppHandle) -> Option<&'static str> {
    let s = core.settings.lock().unwrap().clone();
    match s.provider {
        None => Some("ChooseProvider"),
        Some(Provider::Zense) if !zense::is_signed_in() => Some("SignedOut"),
        Some(Provider::Local)
            if !local::statuses(app)
                .iter()
                .any(|m| m.id == s.local_model && m.downloaded) =>
        {
            Some("ModelNotDownloaded")
        }
        Some(Provider::OpenRouter) if secrets::get(secrets::OPENROUTER_KEY).is_none() => {
            Some("InvalidApiKey")
        }
        Some(Provider::OpenRouter) if s.openrouter_model.is_none() => Some("ChooseModel"),
        _ => None,
    }
}

fn start_recording(app: &AppHandle, core: &Shared) {
    {
        let mut phase = core.phase.lock().unwrap();
        if *phase != Phase::Idle {
            return;
        }
        if let Some(code) = ready_error(core, app) {
            hud(app, "error", Some(code.into()));
            show_settings(app);
            return;
        }
        match audio::start() {
            Ok(rec) => {
                *core.recording.lock().unwrap() = Some((rec, Instant::now()));
                *phase = Phase::Recording;
            }
            Err(code) => {
                hud(app, "error", Some(code));
                return;
            }
        }
    }
    set_tray_recording(app, true);

    let app = app.clone();
    let core = core.clone();
    let lang = ui_lang(&app);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(80));
        let (level, seconds) = {
            let guard = core.recording.lock().unwrap();
            let Some((rec, started)) = guard.as_ref() else { return };
            (rec.level.load(Ordering::Relaxed), started.elapsed().as_secs() as u32)
        };
        let _ = app.emit_to(
            "overlay",
            "hud",
            Hud { state: "recording", code: None, level, seconds, lang: lang.clone() },
        );
        if seconds >= audio::MAX_SECONDS {
            stop_recording(&app, &core);
            return;
        }
    });
}

fn stop_recording(app: &AppHandle, core: &Shared) {
    let rec = {
        let mut phase = core.phase.lock().unwrap();
        if *phase != Phase::Recording {
            return;
        }
        *phase = Phase::Transcribing;
        core.recording.lock().unwrap().take()
    };
    set_tray_recording(app, false);
    let Some((rec, _)) = rec else {
        *core.phase.lock().unwrap() = Phase::Idle;
        return;
    };
    hud(app, "transcribing", None);

    let app = app.clone();
    let core = core.clone();
    tauri::async_runtime::spawn(async move {
        let result = match tauri::async_runtime::spawn_blocking(move || rec.finish()).await {
            Ok(Ok(samples)) => transcribe(&app, &core, samples).await,
            Ok(Err(code)) => Err(code),
            Err(_) => Err("RecorderStopped".into()),
        };
        let restore = core.settings.lock().unwrap().restore_clipboard;
        let outcome = match result {
            Ok(text) if text.trim().is_empty() => Err("NothingHeard".to_string()),
            Ok(text) => {
                let text = text.trim().to_string();
                tauri::async_runtime::spawn_blocking(move || paste::insert(&text, restore))
                    .await
                    .unwrap_or_else(|_| Err("ClipboardUnavailable".into()))
            }
            Err(code) => Err(code),
        };
        match outcome {
            Ok(paste::Outcome::Pasted) => hud(&app, "idle", None),
            Ok(paste::Outcome::CopiedOnly) => hud(&app, "copied", None),
            Err(code) => {
                if code == "SignedOut" {
                    let _ = app.emit("zense-signed-out", ());
                }
                hud(&app, "error", Some(code));
            }
        }
        *core.phase.lock().unwrap() = Phase::Idle;
    });
}

fn script_hint(s: &Settings) -> Option<&'static str> {
    match (s.language.as_str(), s.ui_language.as_deref()) {
        ("zh" | "yue", Some("zh-hans")) => Some("以下是普通话的句子。"),
        ("zh" | "yue", _) => Some("以下是繁體中文的句子。"),
        _ => None,
    }
}

async fn transcribe(app: &AppHandle, core: &Shared, samples: Vec<f32>) -> Result<String, String> {
    if audio::is_silent(&samples) {
        return Err("NothingHeard".into());
    }
    let s = core.settings.lock().unwrap().clone();
    let language = (s.language != "auto").then(|| s.language.clone());
    match s.provider.ok_or("ChooseProvider")? {
        Provider::Zense => {
            let wav = audio::to_wav(&samples)?;
            core.zense.transcribe(&wav, language.as_deref()).await
        }
        Provider::OpenRouter => {
            let key = secrets::get(secrets::OPENROUTER_KEY).ok_or("InvalidApiKey")?;
            let model = s.openrouter_model.clone().ok_or("ChooseModel")?;
            let wav = audio::to_wav(&samples)?;
            openrouter::transcribe(&key, &model, &wav, language.as_deref()).await
        }
        Provider::Local => {
            let app = app.clone();
            let core = core.clone();
            let hint = script_hint(&s);
            tauri::async_runtime::spawn_blocking(move || {
                core.local
                    .transcribe(&app, &s.local_model, &samples, language.as_deref(), hint)
            })
            .await
            .map_err(|_| "TranscriptionFailed".to_string())?
        }
    }
}

fn on_shortcut(app: &AppHandle, state: ShortcutState) {
    let core = app.state::<Shared>().inner().clone();
    let mode = core.settings.lock().unwrap().mode;
    let phase = *core.phase.lock().unwrap();
    if state == ShortcutState::Pressed {
        paste::mask_menu_activation();
    }
    match (mode, state, phase) {
        (RecordMode::Toggle, ShortcutState::Pressed, Phase::Idle) => start_recording(app, &core),
        (RecordMode::Toggle, ShortcutState::Pressed, Phase::Recording) => stop_recording(app, &core),
        (RecordMode::Hold, ShortcutState::Pressed, Phase::Idle) => start_recording(app, &core),
        (RecordMode::Hold, ShortcutState::Released, Phase::Recording) => stop_recording(app, &core),
        _ => {}
    }
}

fn toggle_from_menu(app: &AppHandle) {
    let core = app.state::<Shared>().inner().clone();
    let phase = *core.phase.lock().unwrap();
    match phase {
        Phase::Idle => start_recording(app, &core),
        Phase::Recording => stop_recording(app, &core),
        Phase::Transcribing => {}
    }
}

// ───────────────────────────── commands ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct View {
    settings: Settings,
    platform: &'static str,
    version: &'static str,
    zense_signed_in: bool,
    openrouter_key_set: bool,
    models: Vec<local::ModelStatus>,
    downloading: Option<String>,
    accessibility: bool,
}

#[tauri::command]
fn get_state(app: AppHandle, core: State<Shared>) -> View {
    View {
        settings: core.settings.lock().unwrap().clone(),
        platform: std::env::consts::OS,
        version: env!("CARGO_PKG_VERSION"),
        zense_signed_in: zense::is_signed_in(),
        openrouter_key_set: secrets::get(secrets::OPENROUTER_KEY).is_some(),
        models: local::statuses(&app),
        downloading: core.local.downloading(),
        accessibility: paste::accessibility_granted(),
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, core: State<Shared>, settings: Settings) -> Result<(), String> {
    settings.validate()?;
    let old = core.settings.lock().unwrap().clone();
    if old.shortcut != settings.shortcut {
        let gs = app.global_shortcut();
        gs.register(settings.shortcut.as_str())
            .map_err(|_| "ShortcutUnavailable".to_string())?;
        let _ = gs.unregister(old.shortcut.as_str());
    }
    settings::save(&app, &settings)?;
    *core.settings.lock().unwrap() = settings;
    update_tray_label(&app);
    Ok(())
}

#[tauri::command]
async fn zense_sign_in(app: AppHandle, core: State<'_, Shared>) -> Result<zense::Profile, String> {
    let opener = app.clone();
    core.zense
        .sign_in(move |url| {
            opener
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|_| "BrowserUnavailable".to_string())
        })
        .await
}

#[tauri::command]
async fn zense_profile(core: State<'_, Shared>) -> Result<zense::Profile, String> {
    core.zense.profile().await
}

#[tauri::command]
async fn zense_sign_out(core: State<'_, Shared>) -> Result<(), String> {
    core.zense.sign_out().await;
    Ok(())
}

#[tauri::command]
async fn set_openrouter_key(key: String) -> Result<(), String> {
    let key = key.trim().to_string();
    if key.is_empty() || key.len() > 256 || key.chars().any(char::is_whitespace) {
        return Err("InvalidApiKey".into());
    }
    openrouter::verify_key(&key).await?;
    secrets::set(secrets::OPENROUTER_KEY, &key)
}

#[tauri::command]
fn clear_openrouter_key() {
    secrets::delete(secrets::OPENROUTER_KEY);
}

#[tauri::command]
async fn openrouter_models() -> Result<Vec<openrouter::SttModel>, String> {
    openrouter::list_models().await
}

#[tauri::command]
async fn download_model(app: AppHandle, core: State<'_, Shared>, id: String) -> Result<(), String> {
    core.local.download(&app, &id).await
}

#[tauri::command]
fn cancel_download(core: State<Shared>) {
    core.local.cancel_download();
}

#[tauri::command]
fn delete_model(app: AppHandle, core: State<Shared>, id: String) -> Result<(), String> {
    core.local.delete(&app, &id)
}

#[tauri::command]
fn request_accessibility() -> bool {
    paste::request_accessibility()
}

#[tauri::command]
fn toggle_recording(app: AppHandle) {
    toggle_from_menu(&app);
}

// ───────────────────────────── setup ─────────────────────────────

fn update_tray_label(app: &AppHandle) {
    let shortcut = app.state::<Shared>().settings.lock().unwrap().shortcut.clone();
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(format!("Zpeak — {shortcut}")));
    }
    if let Some(item) = app.try_state::<MenuItem<tauri::Wry>>() {
        let _ = item.set_text(format!("Dictate  ({shortcut})"));
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let dictate = MenuItem::with_id(app, "dictate", "Dictate", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Zpeak", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&dictate, &settings, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    app.manage(dictate);

    let icon = if cfg!(target_os = "macos") {
        Image::from_bytes(include_bytes!("../icons/tray/tray-template.png"))?
    } else {
        Image::from_bytes(include_bytes!("../icons/tray/tray.png"))?
    };
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id.as_ref() {
            "dictate" => toggle_from_menu(app),
            "settings" => show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if !cfg!(target_os = "macos") {
                    show_settings(tray.app_handle());
                }
            }
        })
        .build(app)?;
    update_tray_label(app);
    Ok(())
}

fn build_overlay(app: &AppHandle) -> tauri::Result<()> {
    const W: f64 = 260.0;
    const H: f64 = 64.0;
    let window = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html#overlay".into()))
        .title("Zpeak")
        .inner_size(W, H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .build()?;
    window.set_ignore_cursor_events(true)?;
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let origin = monitor.position().to_logical::<f64>(scale);
        window.set_position(tauri::LogicalPosition::new(
            origin.x + (size.width - W) / 2.0,
            origin.y + size.height - H - 96.0,
        ))?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_settings(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| on_shortcut(app, event.state))
                .build(),
        )
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let settings = settings::load(&handle);
            let shortcut = settings.shortcut.clone();
            let first_run = settings.provider.is_none();
            app.manage::<Shared>(Arc::new(Core {
                settings: Mutex::new(settings),
                phase: Mutex::new(Phase::Idle),
                recording: Mutex::new(None),
                zense: zense::ZenseClient::new(),
                local: local::LocalEngine::default(),
            }));

            if handle.global_shortcut().register(shortcut.as_str()).is_err() {
                log::warn!("shortcut {shortcut} is taken");
            }
            build_tray(&handle)?;
            build_overlay(&handle)?;

            let hidden = std::env::args().any(|a| a == "--hidden");
            if first_run || !hidden {
                show_settings(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            zense_sign_in,
            zense_profile,
            zense_sign_out,
            set_openrouter_key,
            clear_openrouter_key,
            openrouter_models,
            download_model,
            cancel_download,
            delete_model,
            request_accessibility,
            toggle_recording,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Zpeak")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                // Closing the settings window must not quit; only "Quit Zpeak" does.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
