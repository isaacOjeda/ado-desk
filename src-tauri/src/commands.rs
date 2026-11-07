//! Tauri commands: the only surface the webview can call. The PAT enters through
//! `login` and never travels back to the frontend.

use std::sync::Arc;

use tauri::State;

use crate::AppState;
use crate::azdo::AzdoClient;
use crate::azdo::model::{self, PullRequest, Session, WorkItem};
use crate::credentials::{self, Credentials};
use crate::error::{AppError, AppResult};

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

#[tauri::command]
pub async fn login(
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
    Ok(session)
}

/// Signs in with the stored credentials, if any. Returns `None` when signed out.
#[tauri::command]
pub async fn restore_session(state: State<'_, AppState>) -> AppResult<Option<Session>> {
    let Some(stored) = credentials::load()? else {
        return Ok(None);
    };

    let client = AzdoClient::connect(&stored.organization, &stored.pat).await?;
    let session = client.session().clone();
    set_client(&state, Some(Arc::new(client)));
    Ok(Some(session))
}

#[tauri::command]
pub fn logout(state: State<'_, AppState>) -> AppResult<()> {
    set_client(&state, None);
    credentials::delete()
}

#[tauri::command]
pub async fn list_work_items(state: State<'_, AppState>) -> AppResult<Vec<WorkItem>> {
    current_client(&state)?.my_work_items().await
}

#[tauri::command]
pub async fn list_pull_requests(state: State<'_, AppState>) -> AppResult<Vec<PullRequest>> {
    current_client(&state)?.my_pull_requests().await
}
