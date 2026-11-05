//! Commands exposed to the webview. Secrets never leave Rust: the UI only sees masked keys.

use crate::ai::{self, Completion, ModelInfo};
use crate::db::{self, HistoryItem, UsageSummary};
use crate::engine::{self, CommandError};
use crate::flows::{self, PopupPayload};
use crate::settings::{PopupPrefs, ProviderKind, Settings};
use crate::shortcuts::{self, ShortcutFailure};
use crate::state::{AppState, LockExt};
use crate::{actions, secrets, selection, speech, tray, windows};
use serde::Serialize;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

type Res<T> = Result<T, CommandError>;

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings()
}

/// Persist settings, re-register shortcuts and broadcast the change to every window.
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Res<Vec<ShortcutFailure>> {
    let settings = settings.sanitize();
    state
        .store
        .save(&settings)
        .map_err(|e| CommandError::new("io", e.to_string()))?;
    *state.settings.lock_safe() = settings.clone();
    if !settings.history_enabled {
        // Turning history off also forgets what was stored.
        state.db().clear_history()?;
    } else {
        state
            .db()
            .prune_history(engine::now(), settings.history_retention_days)?;
    }
    let failures = shortcuts::apply(&app);
    tray::refresh(&app);
    let _ = app.emit("settings-changed", &settings);
    Ok(failures)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    actions: Vec<actions::ActionInfo>,
    languages: Vec<&'static str>,
}

#[tauri::command]
pub fn get_catalog() -> Catalog {
    Catalog {
        actions: actions::all(),
        languages: actions::LANGUAGES.to_vec(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    configured: bool,
    masked: Option<String>,
}

#[tauri::command]
pub fn get_api_key_status(provider: ProviderKind) -> Res<KeyStatus> {
    let key = secrets::get_key(provider).map_err(|e| CommandError::new("keyring", e))?;
    Ok(KeyStatus {
        configured: key.is_some(),
        masked: key.as_deref().map(secrets::mask),
    })
}

#[tauri::command]
pub fn set_api_key(provider: ProviderKind, key: String) -> Res<KeyStatus> {
    secrets::set_key(provider, &key).map_err(|e| CommandError::new("keyring", e))?;
    get_api_key_status(provider)
}

#[tauri::command]
pub fn delete_api_key(provider: ProviderKind) -> Res<()> {
    secrets::delete_key(provider).map_err(|e| CommandError::new("keyring", e))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTest {
    latency_ms: u64,
    model: String,
    reply: String,
}

/// Same probe as the extension's "Test Connection" button. Not recorded in usage.
#[tauri::command]
pub async fn test_connection(state: State<'_, AppState>) -> Res<ConnectionTest> {
    let settings = state.settings();
    let key = secrets::get_key(settings.provider).map_err(|e| CommandError::new("keyring", e))?;
    let started = Instant::now();
    let c = ai::complete(
        &settings,
        key.as_deref(),
        "Verify connection status. Respond with only 'OK'.",
        "Respond with exactly the single word 'OK' if you receive this message.",
    )
    .await?;
    Ok(ConnectionTest {
        latency_ms: started.elapsed().as_millis() as u64,
        model: settings.model().to_string(),
        reply: c.text.chars().take(40).collect(),
    })
}

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> Res<Vec<ModelInfo>> {
    let settings = state.settings();
    let key = secrets::get_key(settings.provider).map_err(|e| CommandError::new("keyring", e))?;
    Ok(ai::list_models(&settings, key.as_deref()).await?)
}

#[tauri::command]
pub async fn run_action(
    app: AppHandle,
    action: String,
    text: String,
    target_language: Option<String>,
) -> Res<Completion> {
    engine::run(&app, &action, &text, target_language.as_deref(), "popup").await
}

#[tauri::command]
pub fn popup_state(app: AppHandle) -> PopupPayload {
    flows::popup_payload(&app)
}

#[tauri::command]
pub async fn popup_apply(app: AppHandle, text: String) -> Res<String> {
    flows::apply_popup(&app, text)
        .await
        .map(str::to_string)
        .map_err(|e| CommandError::new("paste", e))
}

#[tauri::command]
pub fn popup_close(app: AppHandle) {
    flows::close_popup(&app, true);
}

#[tauri::command]
pub fn popup_prefs(app: AppHandle) -> PopupPrefs {
    flows::popup_prefs(&app)
}

#[tauri::command]
pub fn popup_set_prefs(
    app: AppHandle,
    keep_open: Option<bool>,
    lock_position: Option<bool>,
) -> PopupPrefs {
    flows::set_popup_prefs(&app, keep_open, lock_position)
}

#[tauri::command]
pub async fn speech_info() -> speech::SpeechInfo {
    tauri::async_runtime::spawn_blocking(speech::info)
        .await
        .unwrap_or(speech::SpeechInfo {
            native: false,
            native_languages: Vec::new(),
            system_language: String::new(),
        })
}

/// Start dictation; returns the engine used ("native" | "ai").
#[tauri::command]
pub async fn speech_start(app: AppHandle) -> Res<String> {
    speech::start(&app).await.map(str::to_string)
}

/// Stop dictation; the AI engine returns the transcript.
#[tauri::command]
pub async fn speech_stop(app: AppHandle) -> Res<Option<String>> {
    speech::stop(&app).await
}

#[tauri::command]
pub fn speech_cancel() {
    speech::cancel();
}

/// Windows' speech privacy page ("Online speech recognition").
#[tauri::command]
pub fn open_speech_settings(app: AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    let url = if cfg!(windows) {
        "ms-settings:privacy-speech"
    } else {
        return;
    };
    if let Err(e) = app.opener().open_url(url, None::<&str>) {
        log::warn!("could not open speech settings: {e}");
    }
}

#[tauri::command]
pub fn copy_text(text: String) -> Res<()> {
    selection::write_clipboard(&text).map_err(|e| CommandError::new("clipboard", e.to_string()))
}

#[tauri::command]
pub fn open_settings(app: AppHandle, tab: Option<String>) {
    if let Some(w) = app.get_webview_window(windows::POPUP) {
        let _ = w.hide();
    }
    windows::show_main(&app);
    if let Some(tab) = tab {
        let _ = app.emit_to(windows::MAIN, "navigate", tab);
    }
}

#[tauri::command]
pub fn usage_summary(state: State<AppState>, days: u32, tz_offset_secs: i64) -> Res<UsageSummary> {
    Ok(state
        .db()
        .usage_summary(engine::now(), days, tz_offset_secs)?)
}

#[tauri::command]
pub fn reset_usage(app: AppHandle, state: State<AppState>) -> Res<()> {
    state.db().reset_usage()?;
    let _ = app.emit("usage-changed", ());
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    items: Vec<HistoryItem>,
    total: u64,
}

#[tauri::command]
pub fn list_history(
    state: State<AppState>,
    query: String,
    limit: u32,
    offset: u32,
) -> Res<HistoryPage> {
    let (items, total) = state.db().list_history(&query, limit.min(200), offset)?;
    Ok(HistoryPage { items, total })
}

#[tauri::command]
pub fn delete_history(state: State<AppState>, id: i64) -> Res<()> {
    Ok(state.db().delete_history(id)?)
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) -> Res<()> {
    Ok(state.db().clear_history()?)
}

/// `path` comes from the save dialog the user just confirmed.
#[tauri::command]
pub fn export_history_csv(state: State<AppState>, path: String) -> Res<usize> {
    let items = state.db().all_history()?;
    std::fs::write(&path, db::history_to_csv(&items))
        .map_err(|e| CommandError::new("io", e.to_string()))?;
    Ok(items.len())
}

#[tauri::command]
pub fn shortcut_status(state: State<AppState>) -> Vec<ShortcutFailure> {
    state.shortcut_failures.lock_safe().clone()
}

/// Called while the shortcut recorder is focused so pressing a combo doesn't fire it.
#[tauri::command]
pub fn suspend_shortcuts(app: AppHandle, suspended: bool) -> Vec<ShortcutFailure> {
    if suspended {
        shortcuts::suspend(&app);
        Vec::new()
    } else {
        shortcuts::apply(&app)
    }
}

/// Title-bar minimize button: to the tray or to the taskbar, per settings.
#[tauri::command]
pub fn window_minimize(window: tauri::WebviewWindow, state: State<AppState>) {
    if state.settings().minimize_to_tray {
        let _ = window.hide();
    } else {
        let _ = window.minimize();
    }
}

/// Title-bar close button: hide to the tray, or quit the app.
#[tauri::command]
pub fn window_close(app: AppHandle, window: tauri::WebviewWindow, state: State<AppState>) {
    if state.settings().minimize_to_tray {
        let _ = window.hide();
    } else {
        app.exit(0);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    os: &'static str,
    /// "wayland" | "x11" | "" — global shortcuts/key simulation are limited on Wayland.
    session_type: String,
    data_dir: String,
    log_dir: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    let path = app.path();
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        session_type: std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
        data_dir: path
            .app_data_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        log_dir: path
            .app_log_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}
