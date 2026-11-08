//! Selection button: a small Kaku button shown next to text the user just selected with
//! the mouse. Clicking it opens the palette with that selection, like the shortcut does.
//!
//! There is no system-wide "selection changed" event, so a low-level mouse hook guesses
//! from the gesture: a drag, a double/triple click or a Shift+click that started over a
//! text cursor (I-beam). Nothing is copied until the button is clicked, so selecting
//! text never touches the clipboard.
//!
//! The button window is never activated: it is shown with `SW_SHOWNOACTIVATE` and the
//! hook swallows clicks on it (handling them itself), so the app that owns the selection
//! keeps focus and the palette's Ctrl+C reaches it. Windows only; elsewhere [`apply`] is
//! a no-op and the setting is hidden.

use tauri::AppHandle;

/// Start or stop the hook to match the settings (on, and shortcuts not paused).
pub fn apply(app: &AppHandle) {
    #[cfg(windows)]
    imp::apply(app);
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
mod imp {
    use crate::flows;
    use crate::state::AppState;
    use crate::windows::{self as win, BUBBLE, BUBBLE_SIZE};
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
    use std::sync::mpsc::{self, RecvTimeoutError, Sender};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Manager};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetDoubleClickTime, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CURSORINFO, CallNextHookEx, GA_ROOT, GetAncestor, GetCursorInfo, GetMessageW,
        GetSystemMetrics, GetWindowThreadProcessId, HC_ACTION, HHOOK, HWND_TOPMOST, IDC_IBEAM,
        KBDLLHOOKSTRUCT, LLKHF_INJECTED, LoadCursorW, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
        PeekMessageW, PostThreadMessageW, SM_CXDOUBLECLK, SM_CXDRAG, SM_CYDOUBLECLK, SM_CYDRAG,
        SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos, SetWindowsHookExW, ShowWindow,
        UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MBUTTONDOWN, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN,
        WM_SYSKEYDOWN, WM_XBUTTONDOWN, WindowFromPoint,
    };

    /// The button hides itself when left alone this long (hovering restarts the timer).
    const HIDE_AFTER: Duration = Duration::from_millis(3500);
    /// Distance from the release point, in logical pixels.
    const OFFSET: (f64, f64) = (8.0, 10.0);

    enum Event {
        Show(i32, i32),
        Hide,
        Hover,
        Click,
    }

    /// Thread id of the running hook thread (0 = stopped).
    static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
    static START_LOCK: Mutex<()> = Mutex::new(());
    static EVENTS: OnceLock<Sender<Event>> = OnceLock::new();

    /// Screen rectangle of the visible button, read by the hook without locking.
    static VISIBLE: AtomicBool = AtomicBool::new(false);
    static RECT: [AtomicI32; 4] = [const { AtomicI32::new(0) }; 4];

    pub fn apply(app: &AppHandle) {
        let s = app.state::<AppState>().settings();
        let want = s.selection_bubble && !s.paused;
        let _guard = START_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let running = HOOK_THREAD.load(Ordering::SeqCst);
        if want && running == 0 {
            EVENTS.get_or_init(|| spawn_worker(app.clone()));
            start_hook();
        } else if !want && running != 0 {
            // SAFETY: posting to a thread id we own; fails harmlessly if it already exited.
            let _ = unsafe { PostThreadMessageW(running, WM_QUIT, WPARAM(0), LPARAM(0)) };
            HOOK_THREAD.store(0, Ordering::SeqCst);
            send(Event::Hide);
        }
    }

    fn send(ev: Event) {
        if let Some(tx) = EVENTS.get() {
            let _ = tx.send(ev);
        }
    }

    /// Window work happens here, never inside the hook (Windows drops hooks that stall).
    fn spawn_worker(app: AppHandle) -> Sender<Event> {
        let (tx, rx) = mpsc::channel::<Event>();
        std::thread::Builder::new()
            .name("kaku-bubble".into())
            .spawn(move || {
                let mut deadline: Option<Instant> = None;
                loop {
                    let ev = match deadline {
                        Some(d) => {
                            match rx.recv_timeout(d.saturating_duration_since(Instant::now())) {
                                Ok(ev) => ev,
                                Err(RecvTimeoutError::Timeout) => Event::Hide,
                                Err(RecvTimeoutError::Disconnected) => break,
                            }
                        }
                        None => match rx.recv() {
                            Ok(ev) => ev,
                            Err(_) => break,
                        },
                    };
                    match ev {
                        Event::Show(x, y) => {
                            if show(&app, x, y) {
                                deadline = Some(Instant::now() + HIDE_AFTER);
                            }
                        }
                        Event::Hover if VISIBLE.load(Ordering::SeqCst) => {
                            deadline = Some(Instant::now() + HIDE_AFTER);
                        }
                        Event::Hover => {}
                        Event::Hide => {
                            hide(&app);
                            deadline = None;
                        }
                        Event::Click => {
                            hide(&app);
                            deadline = None;
                            flows::open_popup(app.clone(), true);
                        }
                    }
                }
            })
            .expect("spawn selection button worker");
        tx
    }

    fn bubble_hwnd(app: &AppHandle) -> Option<HWND> {
        app.get_webview_window(BUBBLE)?.hwnd().ok()
    }

    fn show(app: &AppHandle, x: i32, y: i32) -> bool {
        let state = app.state::<AppState>();
        if state.capturing.load(Ordering::SeqCst) || state.busy.load(Ordering::SeqCst) {
            return false;
        }
        let Some(hwnd) = bubble_hwnd(app) else {
            return false;
        };
        let (px, py, scale) = win::fit_near(
            app,
            (f64::from(x), f64::from(y)),
            |scale| (BUBBLE_SIZE.0 * scale, BUBBLE_SIZE.1 * scale),
            OFFSET,
        );
        let w = (BUBBLE_SIZE.0 * scale).round() as i32;
        let h = (BUBBLE_SIZE.1 * scale).round() as i32;
        for (slot, v) in RECT.iter().zip([px, py, px + w, py + h]) {
            slot.store(v, Ordering::SeqCst);
        }
        // SAFETY: a valid top-level window handle owned by this process.
        let shown = unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                px,
                py,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        }
        .is_ok();
        VISIBLE.store(shown, Ordering::SeqCst);
        shown
    }

    fn hide(app: &AppHandle) {
        VISIBLE.store(false, Ordering::SeqCst);
        if let Some(hwnd) = bubble_hwnd(app) {
            // SAFETY: as above; hiding never activates anything.
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
    }

    fn start_hook() {
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();
        let spawned = std::thread::Builder::new()
            .name("kaku-mouse-hook".into())
            .spawn(move || {
                // SAFETY: standard hook setup; the procedures below only read the event
                // structs Windows passes them, and the hooks are removed on this thread.
                unsafe {
                    let module = GetModuleHandleW(None).ok().map(|m| m.into());
                    let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);
                    let keys = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_proc), module, 0);
                    let Ok(mouse) = mouse else {
                        log::warn!("selection button: mouse hook failed: {mouse:?}");
                        if let Ok(k) = keys {
                            let _ = UnhookWindowsHookEx(k);
                        }
                        let _ = ready_tx.send(0);
                        return;
                    };
                    // Create the message queue before anyone posts WM_QUIT to it.
                    let mut msg = MSG::default();
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                    let _ = ready_tx.send(GetCurrentThreadId());
                    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
                    let _ = UnhookWindowsHookEx(mouse);
                    if let Ok(k) = keys {
                        let _ = UnhookWindowsHookEx(k);
                    }
                }
            });
        if spawned.is_err() {
            return;
        }
        if let Ok(tid) = ready_rx.recv_timeout(Duration::from_secs(2))
            && tid != 0
        {
            HOOK_THREAD.store(tid, Ordering::SeqCst);
        }
    }

    // ---- inside the hook: keep it cheap, no window calls on our own windows ----

    #[derive(Default)]
    struct Gesture {
        /// Where the left button went down and whether that looked like text.
        down: Option<(POINT, bool)>,
        /// Last plain click (point, time), to spot double and triple clicks.
        last_click: Option<(POINT, u32)>,
        /// The press started on the button.
        pressed_button: bool,
        hovering: bool,
    }

    thread_local! {
        static GESTURE: RefCell<Gesture> = RefCell::default();
    }

    fn in_button(pt: POINT) -> bool {
        if !VISIBLE.load(Ordering::SeqCst) {
            return false;
        }
        let r = RECT.each_ref().map(|v| v.load(Ordering::SeqCst));
        pt.x >= r[0] && pt.x < r[2] && pt.y >= r[1] && pt.y < r[3]
    }

    fn hide_if_visible() {
        if VISIBLE.load(Ordering::SeqCst) {
            send(Event::Hide);
        }
    }

    /// The cursor is the system text cursor (what browsers, editors and Office use).
    fn over_text() -> bool {
        // SAFETY: plain Win32 queries; CURSORINFO is sized before the call.
        unsafe {
            let Ok(ibeam) = LoadCursorW(None, IDC_IBEAM) else {
                return false;
            };
            let mut info = CURSORINFO {
                cbSize: size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            GetCursorInfo(&mut info).is_ok() && info.hCursor == ibeam
        }
    }

    /// Selections inside Kaku's own windows (palette, settings) never get the button.
    fn own_window(pt: POINT) -> bool {
        // SAFETY: read-only window queries.
        unsafe {
            let hwnd = GetAncestor(WindowFromPoint(pt), GA_ROOT);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            pid == GetCurrentProcessId()
        }
    }

    fn shift_down() -> bool {
        // SAFETY: only reads key state.
        unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) as u16 & 0x8000 != 0 }
    }

    fn metric(x: windows::Win32::UI::WindowsAndMessaging::SYSTEM_METRICS_INDEX) -> i32 {
        // SAFETY: no pointers involved.
        unsafe { GetSystemMetrics(x) }
    }

    /// Returns true to swallow the event.
    fn on_mouse(msg: u32, pt: POINT, time: u32) -> bool {
        GESTURE.with_borrow_mut(|g| match msg {
            WM_MOUSEMOVE => {
                let inside = in_button(pt);
                if inside && !g.hovering {
                    send(Event::Hover);
                }
                g.hovering = inside;
                false
            }
            WM_LBUTTONDOWN => {
                if in_button(pt) {
                    g.pressed_button = true;
                    return true;
                }
                hide_if_visible();
                g.down = Some((pt, over_text() && !own_window(pt)));
                false
            }
            WM_LBUTTONUP => {
                if g.pressed_button {
                    g.pressed_button = false;
                    if in_button(pt) {
                        send(Event::Click);
                    }
                    return true;
                }
                let Some((start, text)) = g.down.take() else {
                    return false;
                };
                if !text {
                    g.last_click = None;
                    return false;
                }
                let drag = (pt.x - start.x).abs() > metric(SM_CXDRAG).max(4) * 2
                    || (pt.y - start.y).abs() > metric(SM_CYDRAG).max(4) * 2;
                // SAFETY: no pointers involved.
                let double_time = unsafe { GetDoubleClickTime() };
                let repeat = g.last_click.is_some_and(|(p, t)| {
                    time.wrapping_sub(t) <= double_time
                        && (pt.x - p.x).abs() <= metric(SM_CXDOUBLECLK)
                        && (pt.y - p.y).abs() <= metric(SM_CYDOUBLECLK)
                });
                g.last_click = (!drag).then_some((pt, time));
                if drag || repeat || shift_down() {
                    send(Event::Show(pt.x, pt.y));
                }
                false
            }
            WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN | WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                g.down = None;
                hide_if_visible();
                false
            }
            _ => false,
        })
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT.
            let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            if on_mouse(wparam.0 as u32, info.pt, info.time) {
                return LRESULT(1);
            }
        }
        // SAFETY: passing the event on unchanged.
        unsafe { CallNextHookEx(None::<HHOOK>, code, wparam, lparam) }
    }

    /// Typing (which replaces the selection) or any shortcut hides the button.
    unsafe extern "system" fn key_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
            // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT.
            let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            // Our own Ctrl+C / Ctrl+V are injected; they must not count as typing.
            if (info.flags & LLKHF_INJECTED).0 == 0 {
                hide_if_visible();
            }
        }
        // SAFETY: passing the event on unchanged.
        unsafe { CallNextHookEx(None::<HHOOK>, code, wparam, lparam) }
    }
}
