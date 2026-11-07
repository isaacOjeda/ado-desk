//! Pull request detail, threads and diffs. STUB: owned by task T3 (odd/tasks/detail-views.md).

use tauri::State;

use crate::AppState;
use crate::error::{AppError, AppResult};

#[tauri::command]
pub async fn get_pull_request(
    _state: State<'_, AppState>,
    _project: String,
    _repository_id: String,
    _id: i64,
) -> AppResult<()> {
    Err(AppError::NotFound)
}

#[tauri::command]
pub async fn list_pull_request_threads(
    _state: State<'_, AppState>,
    _project: String,
    _repository_id: String,
    _id: i64,
) -> AppResult<()> {
    Err(AppError::NotFound)
}

#[tauri::command]
pub async fn list_pull_request_files(
    _state: State<'_, AppState>,
    _project: String,
    _repository_id: String,
    _id: i64,
) -> AppResult<()> {
    Err(AppError::NotFound)
}

#[tauri::command]
pub async fn get_pull_request_file_diff(
    _state: State<'_, AppState>,
    _project: String,
    _repository_id: String,
    _id: i64,
    _path: String,
) -> AppResult<()> {
    Err(AppError::NotFound)
}
