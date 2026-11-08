//! Window management: settings ("main"), action palette ("popup"), progress toast
//! ("toast") and the selection button ("bubble", Windows).

use tauri::{
    AppHandle, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub const MAIN: &str = "main";
pub const POPUP: &str = "popup";
pub const TOAST: &str = "toast";
pub const BUBBLE: &str = "bubble";

pub const POPUP_SIZE: (f64, f64) = (340.0, 372.0);
const TOAST_SIZE: (f64, f64) = (230.0, 48.0);
#[cfg(windows)]
pub const BUBBLE_SIZE: (f64, f64) = (36.0, 36.0);

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

    // Shown and hidden with Win32 calls that never activate it (see bubble.rs).
    #[cfg(windows)]
    WebviewWindowBuilder::new(app, BUBBLE, WebviewUrl::App("bubble.html".into()))
        .title("Kaku")
        .inner_size(BUBBLE_SIZE.0, BUBBLE_SIZE.1)
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

/// Put the palette at its locked spot (when that is still on a screen), else by the cursor.
pub fn place_popup<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    prefs: &crate::settings::PopupPrefs,
) {
    if prefs.lock_position
        && let Some((x, y)) = prefs.position
        && app
            .monitor_from_point(f64::from(x) + 20.0, f64::from(y) + 20.0)
            .ok()
            .flatten()
            .is_some()
    {
        let _ = window.set_position(PhysicalPosition::new(x, y));
        return;
    }
    place_near_cursor(app, window, (12.0, 16.0));
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
    let (x, y, _) = fit_near(
        app,
        (cursor.x, cursor.y),
        |_| (f64::from(size.width), f64::from(size.height)),
        offset,
    );
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Top-left corner (physical pixels) for a window of `size` placed `offset` (logical)
/// away from `point`, flipped to the other side of the point near the work area's edges.
/// `size` receives the monitor's scale factor, which is also returned.
pub fn fit_near<R: Runtime>(
    app: &AppHandle<R>,
    point: (f64, f64),
    size: impl FnOnce(f64) -> (f64, f64),
    offset: (f64, f64),
) -> (i32, i32, f64) {
    let monitor = app
        .monitor_from_point(point.0, point.1)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let (w, h) = size(scale);
    let area = monitor.map(|m| {
        let a = m.work_area();
        (
            f64::from(a.position.x),
            f64::from(a.position.y),
            f64::from(a.size.width),
            f64::from(a.size.height),
        )
    });
    let (x, y) = fit_in_area(point, (w, h), (offset.0 * scale, offset.1 * scale), area);
    (x, y, scale)
}

/// Pure placement math behind [`fit_near`]; `area` is (left, top, width, height).
fn fit_in_area(
    point: (f64, f64),
    (w, h): (f64, f64),
    (dx, dy): (f64, f64),
    area: Option<(f64, f64, f64, f64)>,
) -> (i32, i32) {
    let mut x = point.0 + dx;
    let mut y = point.1 + dy;
    if let Some((left, top, width, height)) = area {
        let (right, bottom) = (left + width, top + height);
        if x + w > right {
            x = (point.0 - w - dx).max(left);
        }
        if y + h > bottom {
            y = (point.1 - h - dy).max(top);
        }
        x = x.clamp(left, (right - w).max(left));
        y = y.clamp(top, (bottom - h).max(top));
    }
    (x.round() as i32, y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::fit_in_area;

    #[test]
    fn placement_flips_at_the_work_area_edges() {
        let area = Some((0.0, 0.0, 1920.0, 1040.0));
        assert_eq!(
            fit_in_area((100.0, 100.0), (36.0, 36.0), (8.0, 10.0), area),
            (108, 110)
        );
        // Bottom-right corner: the window goes up and left of the point instead.
        assert_eq!(
            fit_in_area((1910.0, 1030.0), (36.0, 36.0), (8.0, 10.0), area),
            (1866, 984)
        );
        // Second monitor to the left of the primary one (negative coordinates).
        let left = Some((-1280.0, 0.0, 1280.0, 1000.0));
        assert_eq!(
            fit_in_area((-1279.0, 5.0), (36.0, 36.0), (8.0, 10.0), left),
            (-1271, 15)
        );
        assert_eq!(
            fit_in_area((5.0, 5.0), (36.0, 36.0), (8.0, 10.0), None),
            (13, 15)
        );
    }
}
