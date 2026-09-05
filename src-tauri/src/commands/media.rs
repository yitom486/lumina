//! Media inspection Tauri commands.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::media::{
    list_sibling_videos, tools, MediaError, MediaInfo, MediaInspector, MediaToolStatus,
};

#[tauri::command]
pub async fn media_inspect(app: AppHandle, path: String) -> Result<MediaInfo, MediaError> {
    let resource_dir: Option<PathBuf> = app.path().resource_dir().ok();
    tauri::async_runtime::spawn_blocking(move || {
        MediaInspector::inspect_with(path, resource_dir.as_ref())
    })
    .await
    .map_err(|error| MediaError::internal(Some(&format!("media inspect join: {error}"))))?
}

/// ffprobe presence for startup/settings UI. Never errors: a missing tool is data.
#[tauri::command]
pub async fn media_tool_status(app: AppHandle) -> MediaToolStatus {
    let resource_dir: Option<PathBuf> = app.path().resource_dir().ok();
    match tauri::async_runtime::spawn_blocking(move || tools::tool_status(resource_dir.as_ref()))
        .await
    {
        Ok(status) => status,
        Err(error) => {
            tracing::warn!(%error, "media tool status join failed");
            MediaToolStatus {
                available: false,
                message: MediaError::probe_not_found(None).message,
                hint: None,
            }
        }
    }
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
