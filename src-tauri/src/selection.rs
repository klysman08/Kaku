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
    /// Selected text, or None when the app had nothing selected (or refused to copy).
    pub text: Option<String>,
    /// Where to paste the result back, even when nothing was selected.
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

#[cfg(not(windows))]
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
///
/// Waiting for the physical release used to cost up to a second (people keep Alt down
/// while they look at the screen), so we only wait a short `grace` and then release the
/// keys ourselves. The later physical key-up is harmless: no key-down precedes it.
fn release_modifiers(grace: Duration) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
            VK_RMENU, VK_RSHIFT, VK_RWIN,
        };
        // SAFETY: GetAsyncKeyState only reads key state.
        let down = |vk: u16| unsafe { GetAsyncKeyState(i32::from(vk)) } as u16 & 0x8000 != 0;
        let Ok(mut e) = Enigo::new(&Settings::default()) else {
            return;
        };

        // The hotkey's letter was swallowed by RegisterHotKey, so releasing Alt (or Win)
        // now looks like a lone tap: Windows/Electron apps (Notion, Slack, VS Code...)
        // then activate their menu bar (or the Start menu) and our Ctrl+C is lost.
        // Tapping an unassigned virtual key while the modifier is still down "uses" it,
        // the same trick AutoHotkey calls the menu mask key.
        if down(VK_MENU.0) || down(VK_LWIN.0) || down(VK_RWIN.0) {
            let _ = e.key(Key::Other(MENU_MASK_VK), Direction::Click);
        }

        // Left/right variants, so the key-up matches the physical key that is down.
        let mods = [
            (VK_LSHIFT, Key::LShift),
            (VK_RSHIFT, Key::RShift),
            (VK_LCONTROL, Key::LControl),
            (VK_RCONTROL, Key::RControl),
            (VK_LMENU, Key::LMenu),
            (VK_RMENU, Key::RMenu),
            (VK_LWIN, Key::LWin),
            (VK_RWIN, Key::RWin),
        ];
        let start = Instant::now();
        while mods.iter().any(|(vk, _)| down(vk.0)) && start.elapsed() < grace {
            sleep(Duration::from_millis(10));
        }
        for (vk, key) in mods {
            if down(vk.0) {
                let _ = e.key(key, Direction::Release);
            }
        }
    }
    #[cfg(not(windows))]
    {
        // No portable way to read key state: give the user a moment, then force-release.
        sleep(grace);
        if let Ok(mut e) = Enigo::new(&Settings::default()) {
            for k in [Key::Shift, Key::Alt, Key::Meta, Key::Control] {
                let _ = e.key(k, Direction::Release);
            }
        }
    }
}

/// How long to let the user release the shortcut's modifiers before we do it.
const MODIFIER_GRACE: Duration = Duration::from_millis(120);

/// VK 0xE8 is unassigned by Windows, so tapping it has no effect in any app.
#[cfg(windows)]
const MENU_MASK_VK: u32 = 0xE8;

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

/// How long to wait for the app to put the copied text on the clipboard. Electron
/// apps (Notion, Teams, Slack) write asynchronously and can take well over 500 ms.
pub const COPY_TIMEOUT: Duration = Duration::from_millis(1500);

/// Changes whenever any app writes the clipboard. Polling it is free, whereas opening
/// the clipboard to look at it can make the copying app wait for us.
#[cfg(windows)]
fn clipboard_seq() -> u32 {
    // SAFETY: no arguments, only reads a counter.
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

/// Ctrl+C has been sent to `target`; the text may still be on its way.
pub struct PendingCopy {
    pub target: Target,
    pub previous_clipboard: Option<String>,
    started: Instant,
    #[cfg(windows)]
    seq_before: u32,
}

/// Send Ctrl+C to the focused app without waiting for the result.
pub fn start_copy() -> Result<PendingCopy, SelectionError> {
    let target = foreground_target();
    release_modifiers(MODIFIER_GRACE);
    let previous_clipboard = read_text();
    #[cfg(windows)]
    let seq_before = clipboard_seq();
    // Without a change counter, emptying the clipboard is how we notice the copy.
    #[cfg(not(windows))]
    clear();
    chord('c')?;
    Ok(PendingCopy {
        target,
        previous_clipboard,
        started: Instant::now(),
        #[cfg(windows)]
        seq_before,
    })
}

impl PendingCopy {
    /// Wait until the copied text arrives or `until` (measured from the Ctrl+C) passes.
    pub fn wait(&self, until: Duration) -> Option<String> {
        let deadline = self.started + until;
        loop {
            #[cfg(windows)]
            let changed = clipboard_seq() != self.seq_before;
            #[cfg(not(windows))]
            let changed = true;
            if changed && let Some(text) = read_text() {
                return Some(text);
            }
            if Instant::now() >= deadline {
                return None;
            }
            sleep(Duration::from_millis(10));
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn into_captured(self, text: Option<String>) -> Captured {
        Captured {
            text,
            target: self.target,
            previous_clipboard: self.previous_clipboard,
        }
    }
}

/// Copy the current selection of the focused app. Always returns the target window so
/// the result can be pasted back even when nothing could be copied.
pub fn capture() -> Result<Captured, SelectionError> {
    let pending = start_copy()?;
    let text = pending.wait(COPY_TIMEOUT);
    if text.is_none() {
        log::info!("no text copied from the focused app within {COPY_TIMEOUT:?}");
        restore_if_changed(&pending.previous_clipboard);
    }
    Ok(pending.into_captured(text))
}

/// Put the user's clipboard text back after our Ctrl+C, unless it is already there.
pub fn restore_if_changed(previous: &Option<String>) {
    if let Some(prev) = previous
        && read_text().as_ref() != Some(prev)
    {
        let _ = write_clipboard(prev);
    }
}

/// Paste `text` into `target`, then restore the user's clipboard.
pub fn paste(
    text: &str,
    target: &Target,
    previous_clipboard: Option<String>,
) -> Result<(), SelectionError> {
    write_text_private(text)?;
    focus_target(target);
    release_modifiers(MODIFIER_GRACE);
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
