//! Floating search bar: a small always-on-top window (`search` in tauri.conf.json) shown by a
//! global shortcut from any app. Picking an item hides the bar and opens it in the main window.

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};

use crate::background::{self, MAIN_WINDOW};

pub const SEARCH_WINDOW: &str = "search";
/// Sent to the bar each time it is shown, so it starts with an empty query.
pub const SHOWN_EVENT: &str = "search-bar-shown";
/// Sent to the main window with the item picked in the bar (`SearchHit` JSON).
pub const OPEN_HIT_EVENT: &str = "search-bar-open";
/// Default global shortcut.
pub const DEFAULT_SHORTCUT: &str = "CommandOrControl+Shift+Space";

/// Longest item payload forwarded from the bar to the main window.
pub const MAX_HIT_BYTES: usize = 16 * 1024;

/// Top-left corner for a window of `window` size on a monitor at `origin` with `size`:
/// centered horizontally, a fifth of the way down (all physical pixels).
pub fn bar_position(origin: (i32, i32), size: (u32, u32), window: (u32, u32)) -> (i32, i32) {
    let spare_width = size.0.saturating_sub(window.0) / 2;
    let x = i64::from(origin.0) + i64::from(spare_width);
    let y = i64::from(origin.1) + i64::from(size.1 / 5);
    (clamp_i32(x), clamp_i32(y))
}

fn clamp_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// The bar window, created from its `tauri.conf.json` entry on first use.
fn window(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(window) = app.get_webview_window(SEARCH_WINDOW) {
        return Some(window);
    }
    let config = app.config().app.windows.iter().find(|w| w.label == SEARCH_WINDOW)?.clone();
    WebviewWindowBuilder::from_config(app, &config).ok()?.build().ok()
}

/// Moves the bar to the monitor under the cursor.
fn place(app: &AppHandle, bar: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|cursor| app.monitor_from_point(cursor.x, cursor.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, bar.outer_size()) else {
        let _ = bar.center();
        return;
    };
    let origin = monitor.position();
    let area = monitor.size();
    let (x, y) = bar_position((origin.x, origin.y), (area.width, area.height), (size.width, size.height));
    let _ = bar.set_position(PhysicalPosition::new(x, y));
}

pub fn show(app: &AppHandle) {
    let Some(bar) = window(app) else { return };
    place(app, &bar);
    let _ = bar.show();
    let _ = bar.set_focus();
    let _ = bar.emit_to(SEARCH_WINDOW, SHOWN_EVENT, ());
}

/// Hides the bar. When no other window of the app is visible, the app hides too (macOS), so
/// focus returns to the app the user was in.
pub fn hide(app: &AppHandle) {
    if let Some(bar) = app.get_webview_window(SEARCH_WINDOW) {
        let _ = bar.hide();
    }
    #[cfg(target_os = "macos")]
    {
        let others_visible = app
            .webview_windows()
            .values()
            .any(|w| w.label() != SEARCH_WINDOW && w.is_visible().unwrap_or(false));
        if !others_visible {
            let _ = app.hide();
        }
    }
}

/// The global shortcut: shows the bar, or hides it when it is already in front.
pub fn toggle(app: &AppHandle) {
    let in_front = app
        .get_webview_window(SEARCH_WINDOW)
        .is_some_and(|bar| bar.is_visible().unwrap_or(false) && bar.is_focused().unwrap_or(false));
    if in_front { hide(app) } else { show(app) }
}

/// The bar hides as soon as it loses focus (click elsewhere, app switch).
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() == SEARCH_WINDOW && matches!(event, WindowEvent::Focused(false)) {
        let _ = window.hide();
    }
}

// ---------------------------------------------------------------------------
// Commands (called by the bar window)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn hide_search_bar(app: AppHandle) {
    hide(&app);
}

/// Hides the bar, brings the main window forward and asks it to open `hit`. The main window
/// validates the item before opening it; oversized payloads are dropped here.
#[tauri::command]
pub fn open_search_hit(app: AppHandle, hit: Value) -> Result<(), String> {
    if serde_json::to_vec(&hit).map_or(true, |bytes| bytes.len() > MAX_HIT_BYTES) {
        return Err("This item cannot be opened".into());
    }
    if let Some(bar) = app.get_webview_window(SEARCH_WINDOW) {
        let _ = bar.hide();
    }
    background::show_main(&app);
    app.emit_to(MAIN_WINDOW, OPEN_HIT_EVENT, hit).map_err(|_| "Could not open the item".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_is_centered_in_the_upper_part_of_the_monitor() {
        assert_eq!(bar_position((0, 0), (2000, 1000), (600, 400)), (700, 200));
        // A secondary monitor left of and above the primary one.
        assert_eq!(bar_position((-1920, -200), (1920, 1080), (680, 440)), (-1300, 16));
    }

    #[test]
    fn a_window_wider_than_the_monitor_starts_at_its_left_edge() {
        assert_eq!(bar_position((100, 0), (500, 1000), (800, 400)), (100, 200));
    }
}
