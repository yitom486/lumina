//! On-demand ACP commands. Never runs unless the frontend invokes them.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::acp::model::AgentProfileInput;
use crate::acp::profile::AgentProfile;
use crate::acp::{AcpError, AcpEvent, AcpStatus};
use crate::state::AppState;

#[tauri::command]
pub async fn acp_status(app: AppHandle) -> Result<AcpStatus, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal("应用状态不可用", None));
        };
        Ok(state.acp.status())
    })
    .await
    .map_err(|error| AcpError::internal("ACP 状态查询异常结束", Some(&error.to_string())))?
}

#[tauri::command]
pub async fn acp_set_active_profile(app: AppHandle, id: String) -> Result<AcpStatus, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal("应用状态不可用", None));
        };
        state.acp.set_active_profile(&id)?;
        Ok(state.acp.status())
    })
    .await
    .map_err(|error| AcpError::internal("切换 Agent 任务异常结束", Some(&error.to_string())))?
}

#[tauri::command]
pub async fn acp_upsert_profile(
    app: AppHandle,
    profile: AgentProfileInput,
) -> Result<AgentProfile, AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal("应用状态不可用", None));
        };
        state.acp.upsert_profile(profile)
    })
    .await
    .map_err(|error| AcpError::internal("保存 Agent 配置异常结束", Some(&error.to_string())))?
}

#[tauri::command]
pub async fn acp_prompt(
    state: State<'_, AppState>,
    text: String,
    cwd: Option<String>,
    profile_id: Option<String>,
    on_event: Channel<AcpEvent>,
) -> Result<String, AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || {
        acp.prompt(text, cwd, profile_id, |event| {
            if let Err(error) = on_event.send(event) {
                tracing::warn!(%error, "failed to send ACP event");
            }
        })
    })
    .await
    .map_err(|error| AcpError::internal("ACP 任务异常结束", Some(&error.to_string())))?
}

#[tauri::command]
pub async fn acp_cancel(app: AppHandle) -> Result<(), AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal("应用状态不可用", None));
        };
        state.acp.request_cancel();
        Ok(())
    })
    .await
    .map_err(|error| AcpError::internal("ACP 取消任务异常结束", Some(&error.to_string())))?
}
