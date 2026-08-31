//! Tauri boundary for the local index. These command names double as the
//! application-side tool surface that a future ACP/MCP adapter may expose.

use tauri::State;

use crate::library::{
    LibraryError, LibraryIndex, LibraryStatus, LibraryWatchConfig, PendingMediaGroup,
};
use crate::state::AppState;

#[tauri::command]
pub async fn library_watch_start(
    state: State<'_, AppState>,
    config: LibraryWatchConfig,
) -> Result<LibraryStatus, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.start(config))
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library start join: {error}"))))?
}

#[tauri::command]
pub async fn library_watch_stop(state: State<'_, AppState>) -> Result<LibraryStatus, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.stop())
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library stop join: {error}"))))?
}

#[tauri::command]
pub fn library_status(state: State<'_, AppState>) -> Result<LibraryStatus, LibraryError> {
    state.library.status()
}

#[tauri::command]
pub async fn library_scan_now(
    state: State<'_, AppState>,
) -> Result<Vec<LibraryIndex>, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.scan_now())
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library scan join: {error}"))))?
}

#[tauri::command]
pub async fn library_pending_groups(
    state: State<'_, AppState>,
) -> Result<Vec<PendingMediaGroup>, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.pending_groups())
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library pending join: {error}"))))?
}

#[tauri::command]
pub async fn library_set_manual_title(
    state: State<'_, AppState>,
    root: String,
    group_key: String,
    title: String,
) -> Result<PendingMediaGroup, LibraryError> {
    let service = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || service.set_manual_title(root, group_key, title))
        .await
        .map_err(|error| LibraryError::internal(Some(&format!("library title join: {error}"))))?
}
