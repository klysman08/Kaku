//! Kaku — AI writing actions for any desktop app.

mod actions;
mod ai;
mod commands;
mod db;
mod engine;
mod flows;
mod i18n;
mod secrets;
mod selection;
mod settings;
mod shortcuts;
mod speech;
mod state;
mod tray;
mod windows;

use settings::SettingsStore;
use state::{AppState, LockExt};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64};
use tauri::{Manager, WindowEvent};
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch forwards its args and exits.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            flows::handle_cli(app, &args, false);
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Stdout),
                ])
                .max_file_size(2_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(shortcuts::handle)
                .build(),
        )
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_denylist(&[windows::POPUP, windows::TOAST])
                .build(),
        )
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg("--background")
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            std::fs::create_dir_all(&data_dir)?;

            let store = SettingsStore::new(&config_dir);
            let settings = store.load();
            let popup_prefs = store.load_popup_prefs();
            let db = db::Db::open(&data_dir.join("kaku.db"))?;
            if settings.history_enabled {
                let _ = db.prune_history(engine::now(), settings.history_retention_days);
            }
            let needs_onboarding = !settings.onboarded
                || secrets::get_key(settings.provider).ok().flatten().is_none()
                    && settings.provider == settings::ProviderKind::Gemini;
            let auto_update = settings.auto_update;

            app.manage(AppState {
                settings: Mutex::new(settings),
                store,
                db: Mutex::new(db),
                popup: Mutex::new(None),
                busy: AtomicBool::new(false),
                capturing: AtomicBool::new(false),
                popup_session: AtomicU64::new(0),
                popup_prefs: Mutex::new(popup_prefs),
                shortcut_map: Mutex::new(HashMap::new()),
                shortcut_failures: Mutex::new(Vec::new()),
                last_original: Mutex::new(None),
            });

            windows::create_aux_windows(&handle)?;
            tray::create(&handle)?;

            let failures = shortcuts::apply(&handle);
            if !failures.is_empty() {
                let l = i18n::lang(&handle.state::<AppState>().settings());
                flows::notify(&handle, "Kaku", i18n::tr(l, "shortcutConflict"));
            }

            let args: Vec<String> = std::env::args().collect();
            if needs_onboarding {
                windows::show_main(&handle);
            } else {
                flows::handle_cli(&handle, &args, true);
            }

            if auto_update && !cfg!(debug_assertions) {
                tauri::async_runtime::spawn(check_for_update(handle.clone()));
            }
            Ok(())
        })
        .on_window_event(|window, event| match (window.label(), event) {
            // Closing the settings window keeps the app running in the tray.
            // With "minimize to tray" on, closing keeps the app running in the tray;
            // otherwise closing the settings window quits.
            (windows::MAIN, WindowEvent::CloseRequested { api, .. }) => {
                if minimize_to_tray(window.app_handle()) {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    window.app_handle().exit(0);
                }
            }
            // OS-level minimize (Win+Down, taskbar click) also goes to the tray.
            (windows::MAIN, WindowEvent::Resized(_))
                if window.is_minimized().unwrap_or(false)
                    && minimize_to_tray(window.app_handle()) =>
            {
                let _ = window.hide();
            }
            // Clicking outside the palette dismisses it (without stealing focus back),
            // unless it is kept open. Focus can blink away for a moment while the window
            // is dragged, so look again shortly before closing.
            (windows::POPUP, WindowEvent::Focused(false)) => {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    let keep_open = app.state::<AppState>().popup_prefs.lock_safe().keep_open;
                    if let Some(w) = app.get_webview_window(windows::POPUP)
                        && !keep_open
                        && w.is_visible().unwrap_or(false)
                        && !w.is_focused().unwrap_or(false)
                    {
                        flows::close_popup(&app, false);
                    }
                });
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_catalog,
            commands::get_api_key_status,
            commands::set_api_key,
            commands::delete_api_key,
            commands::test_connection,
            commands::list_models,
            commands::run_action,
            commands::popup_state,
            commands::popup_apply,
            commands::popup_close,
            commands::popup_prefs,
            commands::popup_set_prefs,
            commands::speech_info,
            commands::speech_start,
            commands::speech_stop,
            commands::speech_cancel,
            commands::open_speech_settings,
            commands::copy_text,
            commands::open_settings,
            commands::usage_summary,
            commands::reset_usage,
            commands::list_history,
            commands::delete_history,
            commands::clear_history,
            commands::export_history_csv,
            commands::shortcut_status,
            commands::suspend_shortcuts,
            commands::app_info,
            commands::window_minimize,
            commands::window_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kaku");
}

fn minimize_to_tray(app: &tauri::AppHandle) -> bool {
    app.state::<AppState>().settings().minimize_to_tray
}

/// Background update check: only notifies. Installing happens from Settings › About,
/// so nothing is replaced without the user's click.
async fn check_for_update(app: tauri::AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let Ok(updater) = app.updater() else { return };
    match updater.check().await {
        Ok(Some(update)) => {
            let l = i18n::lang(&app.state::<AppState>().settings());
            flows::notify(
                &app,
                i18n::tr(l, "updateTitle"),
                &format!("{} {}", i18n::tr(l, "updateBody"), update.version),
            );
        }
        Ok(None) => {}
        Err(e) => log::info!("update check failed: {e}"),
    }
}
