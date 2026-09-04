//! yt-dlp Tauri commands (optional online source).

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::ytdl::error::YtdlError;
use crate::ytdl::model::{YtdlInstallEvent, YtdlResolveResult, YtdlStatus};

async fn on_worker<R, F>(app: AppHandle, work: F) -> Result<R, YtdlError>
where
    R: Send + 'static,
    F: FnOnce(&AppState) -> Result<R, YtdlError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(YtdlError::internal(Some("app state unavailable")));
        };
        work(state.inner())
    })
    .await
    .map_err(|error| YtdlError::internal(Some(&format!("ytdl task join: {error}"))))?
}

#[tauri::command]
pub async fn ytdl_status(app: AppHandle) -> Result<YtdlStatus, YtdlError> {
    on_worker(app, |state| Ok(state.ytdl().status())).await
}

#[tauri::command]
pub async fn ytdl_install(
    app: AppHandle,
    on_event: Channel<YtdlInstallEvent>,
) -> Result<YtdlStatus, YtdlError> {
    on_worker(app, move |state| {
        state.ytdl().install(|event| {
            let _ = on_event.send(event);
        })
    })
    .await
}

#[tauri::command]
pub async fn ytdl_resolve(app: AppHandle, url: String) -> Result<YtdlResolveResult, YtdlError> {
    // Service caches full URLs in-process and returns a sanitized IPC copy.
    on_worker(app, move |state| state.ytdl().resolve(&url)).await
}
