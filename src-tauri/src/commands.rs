//! Tauri commands: the only surface the webview can call. PATs enter through
//! `add_organization`/`login` and never travel back to the frontend.

use std::collections::HashMap;
use std::sync::{Arc, RwLockReadGuard, RwLockWriteGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindowBuilder};

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::azdo::model::{self, PullRequest, Session, WorkItem};
use crate::credentials::{self, OrganizationIndex};
use crate::error::{AppError, AppResult};
use crate::settings::{self, Settings, SettingsState};

/// Broadcast to every window when the active organization changes, including sign-in and
/// sign-out (payload: `Session | null`).
pub const SESSION_CHANGED_EVENT: &str = "session-changed";
pub const SETTINGS_WINDOW: &str = "settings";

/// Connected clients by canonical organization name, plus the active one.
/// Organizations other than the active one connect lazily, on switch.
#[derive(Default)]
pub struct Organizations {
    active: Option<String>,
    clients: HashMap<String, Arc<AzdoClient>>,
}

impl Organizations {
    fn active_client(&self) -> Option<Arc<AzdoClient>> {
        self.active.as_ref().and_then(|org| self.clients.get(org)).cloned()
    }

    fn activate(&mut self, client: Arc<AzdoClient>) {
        let organization = client.session().organization.clone();
        self.clients.insert(organization.clone(), client);
        self.active = Some(organization);
    }
}

/// One configured organization as shown in the switcher. Holds no secret.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationSummary {
    pub organization: String,
    pub active: bool,
    /// Known only for organizations connected during this run.
    pub display_name: Option<String>,
    pub email: Option<String>,
}

fn summaries(index: &OrganizationIndex, sessions: &HashMap<String, Session>) -> Vec<OrganizationSummary> {
    index
        .organizations
        .iter()
        .map(|organization| {
            let session = sessions.get(organization);
            OrganizationSummary {
                organization: organization.clone(),
                active: index.active.as_ref() == Some(organization),
                display_name: session.map(|s| s.display_name.clone()),
                email: session.and_then(|s| s.email.clone()),
            }
        })
        .collect()
}

fn organizations(state: &AppState) -> RwLockReadGuard<'_, Organizations> {
    state.organizations.read().unwrap_or_else(|e| e.into_inner())
}

fn organizations_mut(state: &AppState) -> RwLockWriteGuard<'_, Organizations> {
    state.organizations.write().unwrap_or_else(|e| e.into_inner())
}

/// The active organization's client; every Azure DevOps command goes through it.
pub(crate) fn current_client(state: &AppState) -> AppResult<Arc<AzdoClient>> {
    organizations(state).active_client().ok_or(AppError::NotSignedIn)
}

fn broadcast_session(app: &AppHandle, session: Option<&Session>) {
    // Best effort: a window that misses the event still sees the state on its next call.
    let _ = app.emit(SESSION_CHANGED_EVENT, session);
}

/// Parses user input (name or URL) into the canonical lowercase name.
fn canonical_organization(input: &str) -> AppResult<String> {
    Ok(credentials::canonical(&model::parse_organization(input)?))
}

fn not_configured() -> AppError {
    AppError::Credentials("this organization is not configured".into())
}

/// A connected client for `organization`: the cached one, or a new connection with its
/// stored PAT. The state lock is never held across the network call.
async fn client_for(state: &AppState, organization: &str) -> AppResult<Arc<AzdoClient>> {
    let cached = organizations(state).clients.get(organization).cloned();
    if let Some(client) = cached {
        return Ok(client);
    }
    let pat = state.store.pat(organization)?.ok_or_else(not_configured)?;
    Ok(Arc::new(AzdoClient::connect(organization, &pat).await?))
}

/// A client for each of `organizations` (configured names), connecting the ones not connected
/// yet with their stored PAT. New connections are kept for later calls without changing the
/// active organization; an organization that cannot connect keeps its error.
pub(crate) async fn connected_clients(
    state: &AppState,
    organizations: &[String],
) -> Vec<(String, AppResult<Arc<AzdoClient>>)> {
    let connections = futures::future::join_all(organizations.iter().map(|org| client_for(state, org))).await;
    let mut orgs = organizations_mut(state);
    // Read under the lock: `remove` deletes from the store before it takes this lock to drop
    // the client, so a removal either shows here or runs after us and drops what we cache.
    let index = state.store.index().ok();
    organizations
        .iter()
        .cloned()
        .zip(connections)
        .map(|(organization, client)| {
            if let Ok(client) = &client {
                // Skipped when the organization was removed while it was connecting.
                let configured = index.as_ref().is_some_and(|i| i.organizations.contains(&organization));
                if configured {
                    orgs.clients.entry(organization.clone()).or_insert_with(|| client.clone());
                }
            }
            (organization, client)
        })
        .collect()
}

fn activate(app: &AppHandle, state: &AppState, client: Arc<AzdoClient>) -> Session {
    let session = client.session().clone();
    organizations_mut(state).activate(client);
    broadcast_session(app, Some(&session));
    session
}

fn sign_out_in_memory(app: &AppHandle, state: &AppState) {
    organizations_mut(state).active = None;
    broadcast_session(app, None);
}

/// Configured organizations; reads only the non-secret index (no network, no PATs).
#[tauri::command]
pub fn list_organizations(state: State<'_, AppState>) -> AppResult<Vec<OrganizationSummary>> {
    let index = state.store.index()?;
    let sessions: HashMap<String, Session> = organizations(&state)
        .clients
        .iter()
        .map(|(org, client)| (org.clone(), client.session().clone()))
        .collect();
    Ok(summaries(&index, &sessions))
}

/// Validates the PAT, stores it and makes the organization active. Adding an organization
/// that already exists replaces its PAT.
#[tauri::command]
pub async fn add_organization(
    app: AppHandle,
    state: State<'_, AppState>,
    organization: String,
    pat: String,
) -> AppResult<Session> {
    let organization = canonical_organization(&organization)?;
    let pat = pat.trim().to_string();
    if pat.is_empty() {
        return Err(AppError::Unauthorized);
    }

    // Persist only after Azure DevOps accepted the token.
    let client = AzdoClient::connect(&organization, &pat).await?;
    state.store.add(&organization, &pat)?;
    Ok(activate(&app, &state, Arc::new(client)))
}

/// First sign-in from the login screen; same as adding an organization.
#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, AppState>,
    organization: String,
    pat: String,
) -> AppResult<Session> {
    add_organization(app, state, organization, pat).await
}

/// Makes a configured organization active, connecting it if needed. On failure (for
/// example an expired PAT) the previous organization stays active.
#[tauri::command]
pub async fn switch_organization(
    app: AppHandle,
    state: State<'_, AppState>,
    organization: String,
) -> AppResult<Session> {
    let organization = canonical_organization(&organization)?;
    if !state.store.index()?.organizations.contains(&organization) {
        return Err(not_configured());
    }
    let client = client_for(&state, &organization).await?;
    state.store.set_active(&organization)?;
    Ok(activate(&app, &state, client))
}

/// Deletes the organization's PAT. When it was active, the next remaining organization
/// becomes active (returned), or the user is signed out (`None`).
#[tauri::command]
pub async fn remove_organization(
    app: AppHandle,
    state: State<'_, AppState>,
    organization: String,
) -> AppResult<Option<Session>> {
    let organization = canonical_organization(&organization)?;
    remove(&app, &state, &organization).await
}

/// Returns the active session after removing `organization`.
async fn remove(app: &AppHandle, state: &AppState, organization: &str) -> AppResult<Option<Session>> {
    let was_active = organizations(state).active.as_deref() == Some(organization);
    let index = match state.store.remove(organization) {
        Ok(index) => index,
        Err(err) => {
            // Like signing out always did: the active organization is signed out in memory
            // even when its token could not be deleted (the error says so).
            if was_active {
                sign_out_in_memory(app, state);
            }
            return Err(err);
        }
    };
    {
        let mut orgs = organizations_mut(state);
        orgs.clients.remove(organization);
        if was_active {
            orgs.active = None;
        }
    }

    if !was_active {
        return Ok(current_client(state).ok().map(|client| client.session().clone()));
    }
    let Some(next) = index.active else {
        broadcast_session(app, None);
        return Ok(None);
    };
    match client_for(state, &next).await {
        Ok(client) => Ok(Some(activate(app, state, client))),
        // The next organization cannot connect (e.g. expired PAT): sign out; it stays
        // configured and becomes usable again once its token is replaced.
        Err(_) => {
            sign_out_in_memory(app, state);
            Ok(None)
        }
    }
}

/// Signs in to the active organization with its stored PAT. Returns `None` when there is
/// no configured organization.
#[tauri::command]
pub async fn restore_session(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<Session>> {
    let Some(active) = state.store.index()?.active else {
        return Ok(None);
    };
    let pat = state
        .store
        .pat(&active)?
        .ok_or_else(|| AppError::Credentials("the saved token is missing".into()))?;
    let client = AzdoClient::connect(&active, &pat).await?;
    Ok(Some(activate(&app, &state, Arc::new(client))))
}

/// Signs out of the active organization (deleting its PAT) and returns the organization
/// that became active instead, if any.
#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<Session>> {
    let in_memory = organizations(&state).active.clone();
    let active = match in_memory {
        Some(active) => Some(active),
        None => state.store.index()?.active,
    };
    match active {
        Some(organization) => remove(&app, &state, &organization).await,
        None => {
            sign_out_in_memory(&app, &state);
            Ok(None)
        }
    }
}

/// The active session already held in memory; never touches the network or keychain.
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
    state.current()
}

/// Normalizes, persists and broadcasts the settings; returns the stored value. A changed global
/// shortcut is registered first: if it is invalid or taken, nothing is saved and the previous
/// shortcut stays active. Async so the shortcut plugin can hop to the main thread.
#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, SettingsState>,
    settings: Settings,
) -> Result<Settings, String> {
    // Checked before `normalize`, which would silently replace an invalid shortcut.
    if !crate::search_bar::is_valid_shortcut(&settings.search.global_shortcut) {
        return Err(crate::search_bar::INVALID_SHORTCUT.into());
    }
    let settings = settings::normalize(settings);
    {
        // Serializes updates from every window. The settings lock itself is never held while
        // the shortcut plugin waits on the main thread, where `get_settings` takes it.
        let _update = state.updates.lock().unwrap_or_else(|e| e.into_inner());
        let previous = state.current();
        // Only a real change clears a startup problem: an unchanged shortcut registers nothing.
        if crate::search_bar::sync_shortcut(&app, Some(&previous.search), &settings.search)? {
            crate::search_bar::set_shortcut_problem(None);
        }
        if let Err(e) = settings::save(&state.path, &settings) {
            let _ = crate::search_bar::sync_shortcut(&app, Some(&settings.search), &previous.search);
            return Err(format!("Could not save settings: {e}"));
        }
        *state.settings.lock().unwrap_or_else(|e| e.into_inner()) = settings.clone();
        if previous.notifications != settings.notifications
            && let Some(notifications) = app.try_state::<crate::notifications::NotificationsState>()
        {
            notifications.wake();
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::OrganizationStore;
    use crate::credentials::memory::MemoryStore;

    fn session(organization: &str, name: &str) -> Session {
        Session {
            organization: organization.into(),
            user_id: "id".into(),
            display_name: name.into(),
            email: Some(format!("{name}@example.com")),
        }
    }

    #[test]
    fn summaries_mark_the_active_organization_and_known_profiles() {
        let index = OrganizationIndex {
            active: Some("fabrikam".into()),
            organizations: vec!["contoso".into(), "fabrikam".into()],
        };
        let sessions = HashMap::from([("fabrikam".to_string(), session("fabrikam", "ada"))]);

        let list = summaries(&index, &sessions);

        assert_eq!(
            list,
            vec![
                OrganizationSummary {
                    organization: "contoso".into(),
                    active: false,
                    display_name: None,
                    email: None,
                },
                OrganizationSummary {
                    organization: "fabrikam".into(),
                    active: true,
                    display_name: Some("ada".into()),
                    email: Some("ada@example.com".into()),
                },
            ]
        );
    }

    #[test]
    fn organization_list_never_exposes_tokens() {
        let memory = MemoryStore::default();
        let store = OrganizationStore::new(Box::new(memory.clone()));
        store.add("contoso", "super-secret-token").unwrap();
        store.add("fabrikam", "another-secret").unwrap();

        let list = summaries(&store.index().unwrap(), &HashMap::new());
        let json = serde_json::to_string(&list).unwrap();
        let debug = format!("{list:?}");

        assert_eq!(list.len(), 2);
        assert!(json.contains("\"displayName\""));
        for rendered in [json, debug] {
            assert!(!rendered.contains("super-secret-token"));
            assert!(!rendered.contains("another-secret"));
            assert!(!rendered.to_lowercase().contains("pat"));
        }
    }

    #[test]
    fn canonical_organization_lowercases_names_and_urls() {
        assert_eq!(canonical_organization("Contoso").unwrap(), "contoso");
        assert_eq!(canonical_organization("https://dev.azure.com/Smc-Devs/").unwrap(), "smc-devs");
        assert!(canonical_organization("not a name").is_err());
    }
}
