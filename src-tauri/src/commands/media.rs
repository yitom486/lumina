//! Media inspection Tauri commands.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::media::{list_sibling_videos, MediaError, MediaInfo, MediaInspector};

#[tauri::command]
pub async fn media_inspect(
    app: AppHandle,
    path: String,
) -> Result<MediaInfo, MediaError> {
    let resource_dir: Option<PathBuf> = app.path().resource_dir().ok();
    tauri::async_runtime::spawn_blocking(move || {
        MediaInspector::inspect_with(path, resource_dir.as_ref())
    })
    .await
    .map_err(|error| MediaError::internal(Some(&format!("media inspect join: {error}"))))?
}

/// Videos in the same folder as `path` (sorted), for building a playlist.
#[tauri::command]
pub async fn media_list_siblings(path: String) -> Result<Vec<String>, MediaError> {
    tauri::async_runtime::spawn_blocking(move || list_sibling_videos(path))
        .await
        .map_err(|error| {
            MediaError::internal(Some(&format!("media list siblings join: {error}")))
        })?
}
