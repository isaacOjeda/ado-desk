//! Background mode: closing the main window hides it and the app keeps running behind a
//! menu bar (tray) icon, so the global search bar stays available. Quit exits for real.

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, Window, WindowEvent};

use crate::commands;

pub const MAIN_WINDOW: &str = "main";

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

    let mut tray = TrayIconBuilder::with_id("ado-desk")
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
    if window.label() == MAIN_WINDOW
        && TRAY_READY.load(Ordering::Relaxed)
        && let WindowEvent::CloseRequested { api, .. } = event
    {
        api.prevent_close();
        let _ = window.hide();
    }
}
