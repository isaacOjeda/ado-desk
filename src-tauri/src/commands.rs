//! Tauri commands: the only surface the webview can call. The PAT enters through
//! `login` and never travels back to the frontend.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State, WebviewWindowBuilder};

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::azdo::model::{self, PullRequest, Session, WorkItem};
use crate::credentials::{self, Credentials};
use crate::error::{AppError, AppResult};
use crate::settings::{self, Settings, SettingsState};

/// Broadcast to every window when the user signs in or out (payload: `Session | null`).
pub const SESSION_CHANGED_EVENT: &str = "session-changed";
pub const SETTINGS_WINDOW: &str = "settings";

fn set_client(state: &AppState, client: Option<Arc<AzdoClient>>) {
    *state.client.write().unwrap_or_else(|e| e.into_inner()) = client;
}

pub(crate) fn current_client(state: &AppState) -> AppResult<Arc<AzdoClient>> {
    state
        .client
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or(AppError::NotSignedIn)
}

fn broadcast_session(app: &AppHandle, session: Option<&Session>) {
    // Best effort: a window that misses the event still sees the state on its next call.
    let _ = app.emit(SESSION_CHANGED_EVENT, session);
}

#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, AppState>,
    organization: String,
    pat: String,
) -> AppResult<Session> {
    let organization = model::parse_organization(&organization)?;
    let pat = pat.trim().to_string();
    if pat.is_empty() {
        return Err(AppError::Unauthorized);
    }

    // Persist only after Azure DevOps accepted the token.
    let client = AzdoClient::connect(&organization, &pat).await?;
    credentials::save(&Credentials { organization, pat })?;

    let session = client.session().clone();
    set_client(&state, Some(Arc::new(client)));
    broadcast_session(&app, Some(&session));
    Ok(session)
}

/// Signs in with the stored credentials, if any. Returns `None` when signed out.
#[tauri::command]
pub async fn restore_session(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<Session>> {
    let Some(stored) = credentials::load()? else {
        return Ok(None);
    };

    let client = AzdoClient::connect(&stored.organization, &stored.pat).await?;
    let session = client.session().clone();
    set_client(&state, Some(Arc::new(client)));
    broadcast_session(&app, Some(&session));
    Ok(Some(session))
}

#[tauri::command]
pub fn logout(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    set_client(&state, None);
    let deleted = credentials::delete();
    broadcast_session(&app, None);
    deleted
}

/// The signed-in session already held in memory; never touches the network or keychain.
#[tauri::command]
pub fn current_session(state: State<'_, AppState>) -> Option<Session> {
    current_client(&state).ok().map(|client| client.session().clone())
}

#[tauri::command]
pub async fn list_work_items(state: State<'_, AppState>) -> AppResult<Vec<WorkItem>> {
    current_client(&state)?.my_work_items().await
}

#[tauri::command]
pub async fn list_pull_requests(state: State<'_, AppState>) -> AppResult<Vec<PullRequest>> {
    current_client(&state)?.my_pull_requests().await
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Settings {
    state.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Normalizes, persists and broadcasts the settings; returns the stored value.
#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, SettingsState>,
    settings: Settings,
) -> Result<Settings, String> {
    let settings = settings::normalize(settings);
    {
        // Holding the lock across the write serializes updates coming from both windows.
        let mut current = state.settings.lock().unwrap_or_else(|e| e.into_inner());
        settings::save(&state.path, &settings).map_err(|e| format!("Could not save settings: {e}"))?;
        *current = settings.clone();
    }
    let _ = app.emit(settings::CHANGED_EVENT, &settings);
    Ok(settings)
}

/// Focuses the Settings window, creating it from its `tauri.conf.json` entry on first use.
/// Async on purpose: creating a window from a sync command deadlocks on Windows.
#[tauri::command]
pub async fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.unminimize();
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == SETTINGS_WINDOW)
        .cloned()
        .ok_or("The settings window is not configured")?;
    WebviewWindowBuilder::from_config(&app, &config)
        .and_then(|builder| builder.build())
        .map(|_| ())
        .map_err(|e| format!("Could not open settings: {e}"))
}
