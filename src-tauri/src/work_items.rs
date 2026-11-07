//! Work item detail and comments. STUB: owned by task T2 (odd/tasks/detail-views.md).

use tauri::State;

use crate::AppState;
use crate::error::{AppError, AppResult};

#[tauri::command]
pub async fn get_work_item(_state: State<'_, AppState>, _id: i64) -> AppResult<()> {
    Err(AppError::NotFound)
}

#[tauri::command]
pub async fn list_work_item_comments(
    _state: State<'_, AppState>,
    _project: String,
    _id: i64,
) -> AppResult<()> {
    Err(AppError::NotFound)
}
