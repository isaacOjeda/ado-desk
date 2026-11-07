mod azdo;
mod commands;
mod credentials;
mod error;

use std::sync::{Arc, RwLock};

use azdo::AzdoClient;

/// The signed-in client, shared across commands. `None` while signed out.
#[derive(Default)]
pub struct AppState {
    client: RwLock<Option<Arc<AzdoClient>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::login,
            commands::restore_session,
            commands::logout,
            commands::list_work_items,
            commands::list_pull_requests,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
