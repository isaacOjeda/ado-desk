//! Background mode: closing the main window hides it and the app keeps running behind a
//! menu bar (tray) icon, so the global search bar stays available. Quit exits for real.

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, Window, WindowEvent};

use crate::commands;

pub const MAIN_WINDOW: &str = "main";
pub const TRAY_ID: &str = "ado-desk";

/// Whether the menu bar icon exists. Without it, closing the main window quits as before,
/// since there would be no way back on Windows/Linux.
static TRAY_READY: AtomicBool = AtomicBool::new(false);

const OPEN: &str = "open";
const SEARCH: &str = "search";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";

/// Shows, restores and focuses the main window.
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Adds the menu bar icon with its menu.
pub fn install_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id(OPEN, "Open ADO Desk").build(app)?)
        .item(&MenuItemBuilder::with_id(SEARCH, "Search…").build(app)?)
        .item(&MenuItemBuilder::with_id(SETTINGS, "Settings…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(QUIT, "Quit ADO Desk").build(app)?)
        .build()?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("ADO Desk")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN => show_main(app),
            SEARCH => {
                // Like Settings…: the bar window may be created here, so not inside the callback.
                let app = app.clone();
                tauri::async_runtime::spawn(async move { crate::search_bar::show(&app) });
            }
            SETTINGS => {
                // Window creation stays off the menu callback, like the command does.
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = commands::open_settings_window(app).await;
                });
            }
            QUIT => app.exit(0),
            _ => {}
        });
    // Windows convention: a left click opens the app; the menu stays on the right click.
    #[cfg(windows)]
    {
        use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
        tray = tray.show_menu_on_left_click(false).on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    }
    // macOS draws menu bar icons as templates (black plus transparency, tinted by the system
    // for light and dark menu bars); other platforms keep the colored app icon.
    #[cfg(target_os = "macos")]
    {
        tray = tray.icon(tauri::include_image!("icons/tray-icon.png")).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    TRAY_READY.store(true, Ordering::Relaxed);
    Ok(())
}

/// Closing the main window hides it instead of quitting; the search bar hides on blur.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    crate::search_bar::on_window_event(window, event);
    // Without a menu bar icon there is no way back to a hidden app: closing the main window
    // quits, even if the (hidden) search bar window exists.
    if window.label() == MAIN_WINDOW
        && !TRAY_READY.load(Ordering::Relaxed)
        && matches!(event, WindowEvent::CloseRequested { .. })
    {
        window.app_handle().exit(0);
        return;
    }
    if window.label() == MAIN_WINDOW
        && TRAY_READY.load(Ordering::Relaxed)
        && let WindowEvent::CloseRequested { api, .. } = event
    {
        api.prevent_close();
        let _ = window.hide();
    }
}
