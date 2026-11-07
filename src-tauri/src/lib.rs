mod azdo;
mod commands;
mod credentials;
mod error;
mod image_proxy;
mod pull_requests;
mod queries;
mod rich_text;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let path = app.path().app_config_dir()?.join(settings::FILE_NAME);
            app.manage(settings::SettingsState::load(path));
            let themes_path = app.path().app_config_dir()?.join(themes::store::FILE_NAME);
            app.manage(themes::ThemesState::load(themes_path));
            Ok(())
        })
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
            themes::search_theme_extensions,
            themes::import_theme_extension,
            themes::list_imported_themes,
            themes::remove_imported_theme,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
