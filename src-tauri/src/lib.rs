mod azdo;
mod background;
mod commands;
mod credentials;
mod error;
mod image_proxy;
mod pull_requests;
mod queries;
mod rich_text;
mod search;
mod search_bar;
mod settings;
mod themes;
mod work_items;

use std::sync::RwLock;

use tauri::Manager;

/// Shared across commands: the connected organizations (the active one serves every
/// Azure DevOps call) and the keychain-backed organization store.
#[derive(Default)]
pub struct AppState {
    organizations: RwLock<commands::Organizations>,
    store: credentials::OrganizationStore,
}

/// Registers the global search shortcut. A shortcut taken by another app does not stop the
/// app from starting; the bar is then only reachable from the menu bar icon.
fn install_global_shortcut(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    let app = app.clone();
                    // Window work stays off the hotkey callback.
                    tauri::async_runtime::spawn(async move { search_bar::toggle(&app) });
                }
            })
            .build(),
    )?;
    let _ = app.global_shortcut().register(search_bar::DEFAULT_SHORTCUT);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .manage(search::SearchState::default())
        .setup(|app| {
            let path = app.path().app_config_dir()?.join(settings::FILE_NAME);
            app.manage(settings::SettingsState::load(path));
            let themes_path = app.path().app_config_dir()?.join(themes::store::FILE_NAME);
            app.manage(themes::ThemesState::load(themes_path));
            background::install_tray(app.handle())?;
            install_global_shortcut(app.handle())?;
            Ok(())
        })
        .on_window_event(background::on_window_event)
        .register_asynchronous_uri_scheme_protocol(rich_text::PROXY_SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(image_proxy::handle(&app, request).await);
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::login,
            commands::restore_session,
            commands::logout,
            commands::current_session,
            commands::list_organizations,
            commands::add_organization,
            commands::switch_organization,
            commands::remove_organization,
            commands::get_settings,
            commands::update_settings,
            commands::open_settings_window,
            commands::list_work_items,
            commands::list_pull_requests,
            work_items::get_work_item,
            work_items::list_work_item_comments,
            pull_requests::get_pull_request,
            pull_requests::list_pull_request_threads,
            pull_requests::list_pull_request_files,
            pull_requests::get_pull_request_file_diff,
            queries::query_work_items,
            queries::query_pull_requests,
            queries::list_projects,
            queries::list_iterations,
            queries::list_work_item_types,
            queries::list_team_members,
            search::quick_search,
            search_bar::hide_search_bar,
            search_bar::open_search_hit,
            themes::search_theme_extensions,
            themes::import_theme_extension,
            themes::list_imported_themes,
            themes::remove_imported_theme,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Clicking the Dock icon brings back the hidden main window.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows: false, .. } = event {
                background::show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
