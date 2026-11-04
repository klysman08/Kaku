//! Grab the selected text from whatever app is focused and paste a replacement back.
//!
//! There is no cross-app "get selection" API, so we do what users would do by hand:
//! remember the clipboard → send Ctrl+C → read clipboard → … → write result → Ctrl+V →
//! restore the original clipboard. Temporary clipboard writes are excluded from the
//! Windows clipboard history/cloud sync where the OS allows it.
//!
//! All functions block (they sleep while waiting for the target app) — call them from
//! `spawn_blocking`.

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::sync::{LazyLock, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use arboard::SetExtLinux;
#[cfg(windows)]
use arboard::SetExtWindows;

/// One long-lived clipboard handle: on X11 the owner must stay alive to serve pastes.
static CLIPBOARD: LazyLock<Mutex<Option<Clipboard>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, thiserror::Error)]
pub enum SelectionError {
    #[error("no text is selected")]
    NoSelection,
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("keyboard simulation failed: {0}")]
    Input(String),
}

/// The window that had focus when the shortcut fired.
#[derive(Debug, Clone, Copy, Default)]
pub struct Target {
    #[cfg(windows)]
    hwnd: isize,
}

#[derive(Debug, Clone)]
pub struct Captured {
    pub text: String,
    pub target: Target,
    /// Clipboard text before we touched it, restored after pasting.
    pub previous_clipboard: Option<String>,
}

fn with_clipboard<T>(
    f: impl FnOnce(&mut Clipboard) -> Result<T, arboard::Error>,
) -> Result<T, SelectionError> {
    let mut guard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(Clipboard::new().map_err(|e| SelectionError::Clipboard(e.to_string()))?);
    }
    f(guard.as_mut().expect("initialized")).map_err(|e| SelectionError::Clipboard(e.to_string()))
}

fn read_text() -> Option<String> {
    with_clipboard(|c| c.get_text())
        .ok()
        .filter(|t| !t.is_empty())
}

/// Write text without polluting clipboard history / cloud clipboard.
fn write_text_private(text: &str) -> Result<(), SelectionError> {
    with_clipboard(|c| {
        let set = c.set();
        #[cfg(windows)]
        let set = set.exclude_from_history().exclude_from_cloud();
        #[cfg(target_os = "linux")]
        let set = set.exclude_from_history();
        set.text(text.to_owned())
    })
}

fn clear() {
    let _ = with_clipboard(|c| c.clear());
}

pub fn read_clipboard() -> Option<String> {
    read_text()
}

pub fn write_clipboard(text: &str) -> Result<(), SelectionError> {
    with_clipboard(|c| c.set_text(text.to_owned()))
}

pub fn foreground_target() -> Target {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        // SAFETY: plain Win32 call without pointers.
        let hwnd = unsafe { GetForegroundWindow() };
        Target {
            hwnd: hwnd.0 as isize,
        }
    }
    #[cfg(not(windows))]
    Target::default()
}

fn focus_target(target: &Target) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
        if target.hwnd == 0 {
            return;
        }
        // SAFETY: the handle came from GetForegroundWindow; a stale handle just fails.
        unsafe {
            let hwnd = HWND(target.hwnd as *mut _);
            if GetForegroundWindow() != hwnd {
                let _ = SetForegroundWindow(hwnd);
                sleep(Duration::from_millis(80));
            }
        }
    }
    #[cfg(not(windows))]
    let _ = target;
}

/// The user is usually still holding the shortcut's modifiers when we fire; sending
/// Ctrl+C while Shift/Alt is down would trigger a different command in the target app.
fn wait_for_modifiers_release(timeout: Duration) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        };
        let start = Instant::now();
        let held = || {
            [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
                .iter()
                // SAFETY: GetAsyncKeyState only reads key state.
                .any(|vk| unsafe { GetAsyncKeyState(i32::from(vk.0)) } as u16 & 0x8000 != 0)
        };
        while held() && start.elapsed() < timeout {
            sleep(Duration::from_millis(15));
        }
    }
    #[cfg(not(windows))]
    {
        // No portable way to read key state: give the user a moment, then force-release.
        sleep(timeout.min(Duration::from_millis(150)));
        if let Ok(mut e) = Enigo::new(&Settings::default()) {
            for k in [Key::Shift, Key::Alt, Key::Meta, Key::Control] {
                let _ = e.key(k, Direction::Release);
            }
        }
    }
}

fn chord(letter: char) -> Result<(), SelectionError> {
    let err = |e: &dyn std::fmt::Display| SelectionError::Input(e.to_string());
    let mut e = Enigo::new(&Settings::default()).map_err(|x| err(&x))?;
    #[cfg(windows)]
    let key = match letter {
        'c' => Key::C,
        'v' => Key::V,
        other => Key::Unicode(other),
    };
    #[cfg(not(windows))]
    let key = Key::Unicode(letter);
    e.key(Key::Control, Direction::Press).map_err(|x| err(&x))?;
    let r = e.key(key, Direction::Click);
    e.key(Key::Control, Direction::Release)
        .map_err(|x| err(&x))?;
    r.map_err(|x| err(&x))
}

/// Copy the current selection of the focused app.
pub fn capture() -> Result<Captured, SelectionError> {
    let target = foreground_target();
    wait_for_modifiers_release(Duration::from_millis(1200));

    let previous_clipboard = read_text();
    clear();
    chord('c')?;

    let deadline = Instant::now() + Duration::from_millis(700);
    while Instant::now() < deadline {
        sleep(Duration::from_millis(25));
        if let Some(text) = read_text() {
            return Ok(Captured {
                text,
                target,
                previous_clipboard,
            });
        }
    }
    restore(previous_clipboard);
    Err(SelectionError::NoSelection)
}

/// Paste `text` into `target`, then restore the user's clipboard.
pub fn paste(
    text: &str,
    target: &Target,
    previous_clipboard: Option<String>,
) -> Result<(), SelectionError> {
    write_text_private(text)?;
    focus_target(target);
    wait_for_modifiers_release(Duration::from_millis(1200));
    sleep(Duration::from_millis(40));
    chord('v')?;
    // Give the target app time to read the clipboard before restoring it.
    sleep(Duration::from_millis(350));
    if previous_clipboard.is_some() {
        restore(previous_clipboard);
    }
    Ok(())
}

/// Put the user's original clipboard text back (no-op when there was none).
pub fn restore(previous: Option<String>) {
    if let Some(prev) = previous {
        let _ = write_clipboard(&prev);
    }
}

/// Bring the original window back without pasting (popup cancelled).
pub fn refocus(target: &Target) {
    focus_target(target);
}
