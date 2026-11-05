//! User-facing flows triggered by shortcuts, the tray or the command line.

use crate::engine;
use crate::i18n::{lang, tr};
use crate::selection::{self, Captured, SelectionError};
use crate::settings::PopupPrefs;
use crate::state::{AppState, LockExt};
use crate::windows::{self, POPUP, TOAST};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
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
    /// The copy is still on its way (slow app); a "popup:open" with the text follows.
    pub capturing: bool,
}

impl PopupPayload {
    fn from_captured(captured: Option<&Captured>, capturing: bool) -> Self {
        let text = captured.and_then(|c| c.text.clone());
        Self {
            has_selection: text.is_some(),
            text: text.unwrap_or_default(),
            can_replace: captured.is_some(),
            capturing: capturing && captured.is_some_and(|c| c.text.is_none()),
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

/// How long the palette waits for the copied text before it opens anyway. Text from
/// slow apps (Electron) is filled in once it arrives, while the palette is already up.
const QUICK_COPY: Duration = Duration::from_millis(350);

/// Show the palette. From a shortcut (`from_app`) the selection is copied and the focused
/// window remembered for "Replace"; from the tray it opens empty. Pressing the shortcut
/// while the palette has focus closes it; pressing it in another app (palette kept open)
/// takes that app's selection instead.
pub fn open_popup(app: AppHandle, from_app: bool) {
    let Some(win) = app.get_webview_window(POPUP) else {
        return;
    };
    if win.is_visible().unwrap_or(false) && (!from_app || win.is_focused().unwrap_or(false)) {
        close_popup(&app, true);
        return;
    }
    let state = app.state::<AppState>();
    if state.capturing.swap(true, Ordering::SeqCst) {
        return;
    }
    let session = state.popup_session.fetch_add(1, Ordering::SeqCst) + 1;

    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let mut late = None;
        let captured = if from_app {
            let quick = tauri::async_runtime::spawn_blocking(|| {
                selection::start_copy().map(|p| {
                    let text = p.wait(QUICK_COPY);
                    (p, text)
                })
            })
            .await;
            match quick {
                Ok(Ok((pending, Some(text)))) => {
                    selection::restore_if_changed(&pending.previous_clipboard);
                    Some(pending.into_captured(Some(text)))
                }
                Ok(Ok((pending, None))) => {
                    let placeholder = Captured {
                        text: None,
                        target: pending.target,
                        previous_clipboard: pending.previous_clipboard.clone(),
                    };
                    late = Some(pending);
                    Some(placeholder)
                }
                Ok(Err(e)) => {
                    log::warn!("capture failed: {e}");
                    None
                }
                Err(e) => {
                    log::warn!("capture task failed: {e}");
                    None
                }
            }
        } else {
            None
        };
        let copy_ms = started.elapsed().as_millis();

        let payload = PopupPayload::from_captured(captured.as_ref(), late.is_some());
        *app.state::<AppState>().popup.lock_safe() = captured;
        let _ = win.set_size(tauri::LogicalSize::new(
            windows::POPUP_SIZE.0,
            windows::POPUP_SIZE.1,
        ));
        let prefs = app.state::<AppState>().popup_prefs.lock_safe().clone();
        windows::place_popup(&app, &win, &prefs);
        let _ = win.emit("popup:open", &payload);
        let _ = win.show();
        let _ = win.set_focus();
        let selection = match (payload.has_selection, late.is_some()) {
            (true, _) => "yes",
            (false, true) => "pending",
            (false, false) => "no",
        };
        log::info!(
            "palette shown after {}ms (copy {copy_ms}ms, selection: {selection})",
            started.elapsed().as_millis()
        );

        // Stay "capturing" until a slow copy is done: a new copy started meanwhile could
        // mistake our clipboard restore for its own result.
        if let Some(pending) = late {
            finish_late_copy(&app, session, pending).await;
        }
        app.state::<AppState>()
            .capturing
            .store(false, Ordering::SeqCst);
    });
}

/// Keep waiting for a slow copy and hand the text to the palette that asked for it.
async fn finish_late_copy(app: &AppHandle, session: u64, pending: selection::PendingCopy) {
    let joined = tauri::async_runtime::spawn_blocking(move || {
        let text = pending.wait(selection::COPY_TIMEOUT);
        (pending, text)
    })
    .await;
    let Ok((pending, text)) = joined else {
        return;
    };
    let elapsed = pending.elapsed().as_millis();
    selection::restore_if_changed(&pending.previous_clipboard);
    let state = app.state::<AppState>();
    if state.popup_session.load(Ordering::SeqCst) != session {
        return;
    }
    let payload = {
        let mut popup = state.popup.lock_safe();
        let Some(c) = popup.as_mut() else { return };
        c.text = text;
        PopupPayload::from_captured(Some(c), false)
    };
    log::info!(
        "late copy finished after {elapsed}ms (selection: {})",
        if payload.has_selection { "yes" } else { "no" }
    );
    if let Some(w) = app.get_webview_window(POPUP) {
        let _ = w.emit("popup:open", &payload);
    }
}

pub fn popup_payload(app: &AppHandle) -> PopupPayload {
    let state = app.state::<AppState>();
    let popup = state.popup.lock_safe();
    PopupPayload::from_captured(popup.as_ref(), false)
}

/// Hide the palette and (optionally) give focus back. The clipboard was already restored
/// right after the copy.
pub fn close_popup(app: &AppHandle, refocus: bool) {
    crate::speech::cancel();
    let state = app.state::<AppState>();
    state.popup_session.fetch_add(1, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(POPUP) {
        remember_position(app, &w);
        let _ = w.hide();
    }
    let captured = state.popup.lock_safe().take();
    if let Some(c) = captured
        && refocus
    {
        selection::refocus(&c.target);
    }
}

/// Apply the palette result: paste it into the original app (replacing the selection),
/// or, when the palette has no origin window, copy it. Returns "pasted" or "copied".
/// A palette kept open stays on screen, ready for the next selection.
pub async fn apply_popup(app: &AppHandle, text: String) -> Result<&'static str, String> {
    let keep_open = app.state::<AppState>().popup_prefs.lock_safe().keep_open;
    if !keep_open && let Some(w) = app.get_webview_window(POPUP) {
        remember_position(app, &w);
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

pub fn popup_prefs(app: &AppHandle) -> PopupPrefs {
    app.state::<AppState>().popup_prefs.lock_safe().clone()
}

/// Toggle "keep open" / "lock position". Locking pins the palette where it is now.
pub fn set_popup_prefs(
    app: &AppHandle,
    keep_open: Option<bool>,
    lock_position: Option<bool>,
) -> PopupPrefs {
    let state = app.state::<AppState>();
    let prefs = {
        let mut prefs = state.popup_prefs.lock_safe();
        if let Some(v) = keep_open {
            prefs.keep_open = v;
        }
        if let Some(v) = lock_position {
            prefs.lock_position = v;
            if v && let Some(w) = app.get_webview_window(POPUP) {
                prefs.position = w.outer_position().ok().map(|p| (p.x, p.y));
            }
        }
        prefs.clone()
    };
    if let Err(e) = state.store.save_popup_prefs(&prefs) {
        log::warn!("could not save palette preferences: {e}");
    }
    prefs
}

/// While the position is locked, dragging the palette moves the locked spot.
fn remember_position(app: &AppHandle, win: &tauri::WebviewWindow) {
    if !win.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(pos) = win.outer_position() else {
        return;
    };
    let state = app.state::<AppState>();
    let prefs = {
        let mut prefs = state.popup_prefs.lock_safe();
        if !prefs.lock_position || prefs.position == Some((pos.x, pos.y)) {
            return;
        }
        prefs.position = Some((pos.x, pos.y));
        prefs.clone()
    };
    if let Err(e) = state.store.save_popup_prefs(&prefs) {
        log::warn!("could not save palette preferences: {e}");
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
            PopupPayload::from_captured(Some(&copied), false),
            PopupPayload {
                text: "hello".into(),
                has_selection: true,
                can_replace: true,
                capturing: false
            }
        );
        let p = PopupPayload::from_captured(Some(&not_copied), false);
        assert!(!p.has_selection && p.can_replace && p.text.is_empty() && !p.capturing);
        let waiting = PopupPayload::from_captured(Some(&not_copied), true);
        assert!(waiting.capturing && waiting.can_replace);
        let tray = PopupPayload::from_captured(None, true);
        assert!(!tray.has_selection && !tray.can_replace && !tray.capturing);
    }
}
