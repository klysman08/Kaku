//! Global shortcuts: one for the palette, plus optional direct shortcuts per action.

use crate::state::{AppState, LockExt};
use crate::{actions, flows};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

#[derive(Debug, Clone, PartialEq)]
pub enum ShortcutCommand {
    Popup,
    Action(String),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutFailure {
    /// "popup" or an action id.
    pub command: String,
    pub shortcut: String,
    /// "invalid" | "duplicate" | "unavailable"
    pub reason: String,
}

/// Plugin-level handler: look the shortcut up and dispatch on key release
/// (so the main key is no longer down when we simulate Ctrl+C).
pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() != ShortcutState::Released {
        return;
    }
    let state = app.state::<AppState>();
    let command = state.shortcut_map.lock_safe().get(&shortcut.id()).cloned();
    match command {
        Some(ShortcutCommand::Popup) => flows::open_popup(app.clone(), true),
        Some(ShortcutCommand::Action(id)) => flows::run_direct(app.clone(), id, false),
        None => {}
    }
}

/// (Re)register everything from settings, including the selection button. Returns the
/// shortcuts that failed.
pub fn apply(app: &AppHandle) -> Vec<ShortcutFailure> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let gs = app.global_shortcut();
    if let Err(e) = gs.unregister_all() {
        log::warn!("unregister_all failed: {e}");
    }
    let mut map = HashMap::new();
    let mut failures = Vec::new();

    if !settings.paused {
        let mut wanted: Vec<(String, String, ShortcutCommand)> = vec![(
            "popup".into(),
            settings.shortcuts.popup.clone(),
            ShortcutCommand::Popup,
        )];
        for (id, combo) in &settings.shortcuts.actions {
            if actions::is_valid(id) {
                wanted.push((
                    id.clone(),
                    combo.clone(),
                    ShortcutCommand::Action(id.clone()),
                ));
            }
        }
        let mut seen = HashSet::new();
        for (name, combo, cmd) in wanted {
            if combo.trim().is_empty() {
                continue;
            }
            let fail = |reason: &str| ShortcutFailure {
                command: name.clone(),
                shortcut: combo.clone(),
                reason: reason.into(),
            };
            let Ok(sc) = Shortcut::from_str(&combo) else {
                failures.push(fail("invalid"));
                continue;
            };
            if !seen.insert(sc.id()) {
                failures.push(fail("duplicate"));
                continue;
            }
            match gs.register(sc) {
                Ok(()) => {
                    map.insert(sc.id(), cmd);
                }
                Err(e) => {
                    log::warn!("could not register {combo}: {e}");
                    failures.push(fail("unavailable"));
                }
            }
        }
    }

    *state.shortcut_map.lock_safe() = map;
    *state.shortcut_failures.lock_safe() = failures.clone();
    // The selection button follows the same settings and the same pause switch.
    crate::bubble::apply(app);
    failures
}

/// Unregister everything temporarily (while the user records a new combination).
pub fn suspend(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    app.state::<AppState>().shortcut_map.lock_safe().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_recorder_format() {
        for s in [
            "Ctrl+Shift+Space",
            "Ctrl+Shift+1",
            "Alt+Shift+K",
            "Ctrl+Alt+F9",
            "Super+Shift+T",
        ] {
            assert!(Shortcut::from_str(s).is_ok(), "{s}");
        }
        assert!(Shortcut::from_str("Ctrl+").is_err());
        assert_eq!(
            Shortcut::from_str("ctrl+shift+1").unwrap().id(),
            Shortcut::from_str("Shift+Ctrl+Digit1").unwrap().id()
        );
    }
}
