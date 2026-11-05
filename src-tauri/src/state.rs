use crate::db::Db;
use crate::selection::Captured;
use crate::settings::{PopupPrefs, Settings, SettingsStore};
use crate::shortcuts::{ShortcutCommand, ShortcutFailure};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64};

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub store: SettingsStore,
    pub db: Mutex<Db>,
    /// Selection captured when the palette opened (None = clipboard mode, nothing selected).
    pub popup: Mutex<Option<Captured>>,
    /// Set while a direct-shortcut action is running (prevents double triggers).
    pub busy: AtomicBool,
    /// Set while the palette is copying the selection (prevents double triggers).
    pub capturing: AtomicBool,
    /// Bumped on every palette open, so a late copy never lands in a newer palette.
    pub popup_session: AtomicU64,
    pub popup_prefs: Mutex<PopupPrefs>,
    /// Global shortcut id → command.
    pub shortcut_map: Mutex<HashMap<u32, ShortcutCommand>>,
    pub shortcut_failures: Mutex<Vec<ShortcutFailure>>,
    /// Original text of the last replacement (tray: "copy original text").
    pub last_original: Mutex<Option<String>>,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn db(&self) -> std::sync::MutexGuard<'_, Db> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Small helper so poisoned mutexes never take the app down.
pub trait LockExt<T> {
    fn lock_safe(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_safe(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}
