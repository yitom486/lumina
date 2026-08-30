//! On-demand ASR commands. Never runs unless the frontend invokes them.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::asr::{AsrError, AsrEvent, AsrStatus};
use crate::state::AppState;
use crate::subtitle::Transcript;

/// Path probe only — off the UI thread (directory scan).
#[tauri::command]
pub async fn asr_status(app: AppHandle) -> Result<AsrStatus, AsrError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AsrError::internal("app state is not available", None));
        };
        Ok(state.asr.status())
    })
    .await
    .map_err(|error| AsrError::internal("asr_status join failed", Some(&error.to_string())))?
}

/// Blocking work runs on a worker thread so the UI stays responsive.
/// Whisper/model is touched only inside this call path.
#[tauri::command]
pub async fn asr_transcribe(
    state: State<'_, AppState>,
    path: String,
    on_event: Channel<AsrEvent>,
) -> Result<Transcript, AsrError> {
    let asr = state.asr.clone();
    let path_for_job = path.clone();

    tauri::async_runtime::spawn_blocking(move || {
        asr.transcribe(path_for_job, |event| {
            if let Err(error) = on_event.send(event) {
                tracing::warn!(%error, "failed to send ASR event");
            }
        })
    })
    .await
    .map_err(|error| AsrError::internal("ASR worker join failed", Some(&error.to_string())))?
}
