//! User-facing flows triggered by shortcuts, the tray or the command line.

use crate::engine;
use crate::i18n::{lang, tr};
use crate::selection::{self, Captured, SelectionError};
use crate::state::{AppState, LockExt};
use crate::windows::{self, POPUP, TOAST};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PopupPayload {
    /// Text copied from the app ("" when nothing could be copied).
    pub text: String,
    /// True when the selection was copied automatically.
    pub has_selection: bool,
    /// True when we know which app/window to paste back into (palette opened by shortcut).
    /// Even without a copied selection, "Replace" then pastes over whatever is still
    /// selected there.
    pub can_replace: bool,
}

impl PopupPayload {
    fn from_captured(captured: Option<&Captured>) -> Self {
        let text = captured.and_then(|c| c.text.clone());
        Self {
            has_selection: text.is_some(),
            text: text.unwrap_or_default(),
            can_replace: captured.is_some(),
        }
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("notification failed: {e}");
    }
}

fn notify_error(app: &AppHandle, err: &engine::CommandError) {
    let l = lang(&app.state::<AppState>().settings());
    let body = if err.code == "missing_key" {
        tr(l, "missingKey").to_string()
    } else {
        err.message.clone()
    };
    notify(app, tr(l, "errorTitle"), &body);
}

fn show_toast(app: &AppHandle, action: &str) {
    if let Some(w) = app.get_webview_window(TOAST) {
        windows::place_near_cursor(app, &w, (18.0, 22.0));
        let _ = w.emit("toast:show", action);
        let _ = w.show();
    }
}

fn hide_toast(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(TOAST) {
        let _ = w.hide();
    }
}

async fn capture() -> Result<Captured, SelectionError> {
    tauri::async_runtime::spawn_blocking(selection::capture)
        .await
        .unwrap_or(Err(SelectionError::Input("capture task panicked".into())))
}

async fn paste(text: String, captured: Captured) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        selection::paste(&text, &captured.target, captured.previous_clipboard)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Direct shortcut: transform the selection in place. With `clipboard_mode` the text
/// comes from, and goes back to, the clipboard (Wayland / CLI fallback).
pub fn run_direct(app: AppHandle, action: String, clipboard_mode: bool) {
    let state = app.state::<AppState>();
    if state.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let l = lang(&app.state::<AppState>().settings());
        let captured = if clipboard_mode {
            Ok(Captured {
                text: selection::read_clipboard(),
                target: Default::default(),
                previous_clipboard: None,
            })
        } else {
            capture().await
        };

        match captured {
            Err(e) => {
                log::warn!("capture failed: {e}");
                notify(&app, "Kaku", tr(l, "noSelection"));
            }
            Ok(Captured { text: None, .. }) => notify(&app, "Kaku", tr(l, "noSelection")),
            Ok(cap) => {
                let original = cap.text.clone().unwrap_or_default();
                show_toast(&app, &action);
                let source = if clipboard_mode { "cli" } else { "shortcut" };
                let result = engine::run(&app, &action, &original, None, source).await;
                hide_toast(&app);
                match result {
                    Ok(done) => {
                        *app.state::<AppState>().last_original.lock_safe() = Some(original);
                        let _ = app.emit("action-done", &action);
                        if clipboard_mode {
                            let _ = selection::write_clipboard(&done.text);
                            notify(&app, "Kaku", tr(l, "clipboardDone"));
                        } else if let Err(e) = paste(done.text, cap).await {
                            log::error!("paste failed: {e}");
                        }
                    }
                    Err(e) => {
                        selection::restore(cap.previous_clipboard);
                        notify_error(&app, &e);
                    }
                }
            }
        }
        app.state::<AppState>().busy.store(false, Ordering::SeqCst);
    });
}

/// Show the palette by the cursor. From a shortcut (`from_app`) the selection is copied
/// and the focused window remembered for "Replace"; from the tray it opens empty.
pub fn open_popup(app: AppHandle, from_app: bool) {
    let Some(win) = app.get_webview_window(POPUP) else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        close_popup(&app, true);
        return;
    }
    tauri::async_runtime::spawn(async move {
        let captured = if from_app {
            match capture().await {
                Ok(c) => Some(c),
                Err(e) => {
                    log::warn!("capture failed: {e}");
                    None
                }
            }
        } else {
            None
        };
        let payload = PopupPayload::from_captured(captured.as_ref());
        *app.state::<AppState>().popup.lock_safe() = captured;
        let _ = win.set_size(tauri::LogicalSize::new(
            windows::POPUP_SIZE.0,
            windows::POPUP_SIZE.1,
        ));
        windows::place_near_cursor(&app, &win, (12.0, 16.0));
        let _ = win.emit("popup:open", &payload);
        let _ = win.show();
        let _ = win.set_focus();
    });
}

pub fn popup_payload(app: &AppHandle) -> PopupPayload {
    let state = app.state::<AppState>();
    let popup = state.popup.lock_safe();
    PopupPayload::from_captured(popup.as_ref())
}

/// Hide the palette, restore the clipboard and (optionally) give focus back.
pub fn close_popup(app: &AppHandle, refocus: bool) {
    if let Some(w) = app.get_webview_window(POPUP) {
        let _ = w.hide();
    }
    let captured = app.state::<AppState>().popup.lock_safe().take();
    if let Some(c) = captured {
        selection::restore(c.previous_clipboard.clone());
        if refocus {
            selection::refocus(&c.target);
        }
    }
}

/// Apply the palette result: paste it into the original app (replacing the selection),
/// or, when the palette has no origin window, copy it. Returns "pasted" or "copied".
pub async fn apply_popup(app: &AppHandle, text: String) -> Result<&'static str, String> {
    if let Some(w) = app.get_webview_window(POPUP) {
        let _ = w.hide();
    }
    let captured = app.state::<AppState>().popup.lock_safe().take();
    match captured {
        Some(c) => {
            if let Some(original) = c.text.clone() {
                *app.state::<AppState>().last_original.lock_safe() = Some(original);
            }
            paste(text, c).await?;
            Ok("pasted")
        }
        None => {
            selection::write_clipboard(&text).map_err(|e| e.to_string())?;
            Ok("copied")
        }
    }
}

/// Command line: `kaku [--action <id> [--clipboard]] [--popup] [--settings] [--background]`.
/// Also used for arguments forwarded by a second instance.
pub fn handle_cli(app: &AppHandle, args: &[String], first_launch: bool) {
    let mut iter = args.iter().skip(1);
    let mut action = None;
    let mut clipboard = false;
    let mut popup = false;
    let mut settings = false;
    let mut background = false;
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--action" | "-a" => action = iter.next().cloned(),
            "--clipboard" | "-c" => clipboard = true,
            "--popup" | "-p" => popup = true,
            "--settings" | "-s" => settings = true,
            "--background" | "--minimized" => background = true,
            _ => {}
        }
    }
    if let Some(id) = action {
        if crate::actions::is_valid(&id) {
            run_direct(app.clone(), id, clipboard);
        } else {
            log::warn!("unknown action from CLI: {id}");
        }
    } else if popup {
        open_popup(app.clone(), true);
    } else if settings || !background || !first_launch {
        windows::show_main(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_flags_are_documented() {
        // Keep README and --help text in sync with these flags.
        let readme = include_str!("../../README.md");
        for flag in ["--action", "--clipboard", "--popup", "--settings"] {
            assert!(readme.contains(flag), "README is missing {flag}");
        }
    }

    #[test]
    fn popup_payload_offers_replace_whenever_origin_is_known() {
        let copied = Captured {
            text: Some("hello".into()),
            target: Default::default(),
            previous_clipboard: None,
        };
        let not_copied = Captured {
            text: None,
            ..copied.clone()
        };
        assert_eq!(
            PopupPayload::from_captured(Some(&copied)),
            PopupPayload {
                text: "hello".into(),
                has_selection: true,
                can_replace: true
            }
        );
        let p = PopupPayload::from_captured(Some(&not_copied));
        assert!(!p.has_selection && p.can_replace && p.text.is_empty());
        let tray = PopupPayload::from_captured(None);
        assert!(!tray.has_selection && !tray.can_replace);
    }
}
