//! Window management: settings ("main"), action palette ("popup"), progress toast ("toast").

use tauri::{
    AppHandle, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub const MAIN: &str = "main";
pub const POPUP: &str = "popup";
pub const TOAST: &str = "toast";

pub const POPUP_SIZE: (f64, f64) = (340.0, 372.0);
const TOAST_SIZE: (f64, f64) = (230.0, 48.0);

pub fn create_aux_windows(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, POPUP, WebviewUrl::App("popup.html".into()))
        .title("Kaku")
        .inner_size(POPUP_SIZE.0, POPUP_SIZE.1)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;

    WebviewWindowBuilder::new(app, TOAST, WebviewUrl::App("toast.html".into()))
        .title("Kaku")
        .inner_size(TOAST_SIZE.0, TOAST_SIZE.1)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Place a window next to the mouse cursor, kept inside the monitor's work area.
pub fn place_near_cursor<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    offset: (f64, f64),
) {
    let Ok(cursor) = app.cursor_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let mut x = cursor.x + offset.0 * scale;
    let mut y = cursor.y + offset.1 * scale;
    if let Some(m) = monitor {
        let area = m.work_area();
        let (left, top) = (f64::from(area.position.x), f64::from(area.position.y));
        let right = left + f64::from(area.size.width);
        let bottom = top + f64::from(area.size.height);
        let (w, h) = (f64::from(size.width), f64::from(size.height));
        if x + w > right {
            x = (cursor.x - w - offset.0 * scale).max(left);
        }
        if y + h > bottom {
            y = (cursor.y - h - offset.1 * scale).max(top);
        }
        x = x.clamp(left, (right - w).max(left));
        y = y.clamp(top, (bottom - h).max(top));
    }
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}
