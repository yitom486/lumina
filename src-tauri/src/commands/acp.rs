//! On-demand ACP commands. Never runs unless the frontend invokes them.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::acp::settings::AcpClientSettings;
use crate::acp::{
    AcpError, AcpEvent, AcpStatus, AgentProfilesHint, SavedSessionHint, VideoPromptContext,
};
use crate::state::AppState;

#[tauri::command]
pub async fn acp_status(
    app: AppHandle,
    profiles: AgentProfilesHint,
) -> Result<AcpStatus, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        Ok(state.acp.status(&profiles))
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp status join: {error}"))))?
}

#[tauri::command]
pub async fn acp_respond_permission(
    app: AppHandle,
    request_id: String,
    option_id: Option<String>,
) -> Result<(), AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        state.acp.respond_permission(&request_id, option_id)
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp respond permission join: {error}"))))?
}

#[tauri::command]
pub async fn acp_connect(
    state: State<'_, AppState>,
    cwd: Option<String>,
    profile_id: Option<String>,
    saved_session: Option<SavedSessionHint>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        acp.connect(
            cwd,
            profile_id,
            saved_session,
            settings,
            profiles,
            |event| {
                if let Err(error) = on_event.send(event) {
                    tracing::warn!(%error, "failed to send ACP connect event");
                }
            },
        )
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp connect join: {error}"))))?
}

#[tauri::command]
// Tauri maps IPC payload fields to command arguments directly.
#[allow(clippy::too_many_arguments)]
pub async fn acp_prompt(
    state: State<'_, AppState>,
    text: String,
    cwd: Option<String>,
    profile_id: Option<String>,
    context: Option<VideoPromptContext>,
    saved_session: Option<SavedSessionHint>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<String, AcpError> {
    let acp = state.acp.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        acp.prompt(
            text,
            cwd,
            profile_id,
            context,
            saved_session,
            settings,
            profiles,
            |event| {
                if let Err(error) = on_event.send(event) {
                    tracing::warn!(%error, "failed to send ACP event");
                }
            },
        )
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
