use crate::i18n::{lang, tr};
use crate::state::{AppState, LockExt};
use crate::{flows, selection, shortcuts, windows};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

pub const TRAY_ID: &str = "main-tray";

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let settings = app.state::<AppState>().settings();
    let l = lang(&settings);
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", tr(l, "open"), true, None::<&str>)?,
            &MenuItem::with_id(app, "palette", tr(l, "palette"), true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(
                app,
                "pause",
                tr(l, "pause"),
                true,
                settings.paused,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app,
                "copy-original",
                tr(l, "copyOriginal"),
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", tr(l, "quit"), true, None::<&str>)?,
        ],
    )
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Kaku")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => windows::show_main(app),
            "palette" => flows::open_popup(app.clone(), false),
            "pause" => toggle_pause(app),
            "copy-original" => {
                let original = app.state::<AppState>().last_original.lock_safe().clone();
                if let Some(text) = original {
                    let _ = selection::write_clipboard(&text);
                }
            }
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
                windows::show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuild the menu (language or pause state changed).
pub fn refresh(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn toggle_pause(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut s = state.settings.lock_safe();
        s.paused = !s.paused;
        s.clone()
    };
    if let Err(e) = state.store.save(&snapshot) {
        log::error!("could not save settings: {e}");
    }
    shortcuts::apply(app);
    refresh(app);
    let _ = app.emit("settings-changed", &snapshot);
}
