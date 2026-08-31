//! On-demand ACP commands. Never runs unless the frontend invokes them.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::acp::model::AgentProfileInput;
use crate::acp::profile::AgentProfile;
use crate::acp::{AcpError, AcpEvent, AcpStatus, VideoPromptContext};
use crate::state::AppState;

#[tauri::command]
pub async fn acp_status(app: AppHandle) -> Result<AcpStatus, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        Ok(state.acp.status())
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp status join: {error}"))))?
}

#[tauri::command]
pub async fn acp_set_active_profile(app: AppHandle, id: String) -> Result<AcpStatus, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        state.acp.set_active_profile(&id)?;
        Ok(state.acp.status())
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp set active join: {error}"))))?
}

#[tauri::command]
pub async fn acp_upsert_profile(
    app: AppHandle,
    profile: AgentProfileInput,
) -> Result<AgentProfile, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        state.acp.upsert_profile(profile)
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp upsert join: {error}"))))?
}

#[tauri::command]
pub async fn acp_prompt(
    state: State<'_, AppState>,
    text: String,
    cwd: Option<String>,
    profile_id: Option<String>,
    context: Option<VideoPromptContext>,
    on_event: Channel<AcpEvent>,
) -> Result<String, AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || {
        acp.prompt(text, cwd, profile_id, context, |event| {
            if let Err(error) = on_event.send(event) {
                tracing::warn!(%error, "failed to send ACP event");
            }
        })
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp prompt join: {error}"))))?
}

#[tauri::command]
pub async fn acp_cancel(app: AppHandle) -> Result<(), AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        state.acp.request_cancel();
        Ok(())
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp cancel join: {error}"))))?
}

#[tauri::command]
pub async fn acp_close(app: AppHandle) -> Result<(), AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        state.acp.close_session()
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp close join: {error}"))))?
}
