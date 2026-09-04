//! On-demand ACP commands. Never runs unless the frontend invokes them.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::acp::paths::resolve_session_cwd;
use crate::acp::settings::AcpClientSettings;
use crate::acp::{
    AcpError, AcpEvent, AcpStatus, AgentProfilesHint, SavedSessionHint, VideoPromptContext,
};
use crate::mcp::{snapshot_path_for_cwd, OnlineMediaSnapshot};
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
pub async fn acp_sync_mcp_capabilities(
    state: State<'_, AppState>,
    cwd: Option<String>,
    client_settings: Option<AcpClientSettings>,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        acp.sync_mcp_capabilities(cwd.as_deref(), settings.vision_capable)
    })
    .await
    .map_err(|error| {
        AcpError::internal(Some(&format!("acp sync mcp capabilities join: {error}")))
    })??;
    Ok(())
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
    history_context: Option<String>,
    saved_session: Option<SavedSessionHint>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<String, AcpError> {
    let acp = state.acp.clone();
    let library = state.library.clone();
    let ytdl = state.ytdl.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        let session_cwd = resolve_session_cwd(cwd.as_deref())?;
        let mut snapshot =
            acp.build_prompt_snapshot(context.as_ref(), &library, settings.vision_capable)?;
        if let Some(page_url) = context
            .as_ref()
            .and_then(|value| value.media_path.as_deref())
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
        {
            match ytdl.cached_resolve(page_url) {
                Ok(Some(resolved)) => {
                    let choices = crate::ytdl::subtitle::list_choices(&resolved);
                    let selected = context
                        .as_ref()
                        .and_then(|value| value.subtitle_choice_id.as_deref())
                        .filter(|id| choices.iter().any(|choice| choice.id == *id))
                        .map(str::to_string)
                        .or_else(|| choices.first().map(|choice| choice.id.clone()));
                    let transcript = selected.as_deref().and_then(|choice_id| {
                        match ytdl.load_subtitle_choice(page_url, choice_id) {
                            Ok(transcript) => Some(transcript),
                            Err(error) => {
                                tracing::warn!(
                                    code = ?error.code,
                                    details = ?error.details,
                                    "online transcript unavailable for ACP snapshot"
                                );
                                None
                            }
                        }
                    });
                    if let Some(anchor) = snapshot.anchor.as_mut() {
                        anchor.subtitle_choice_id = selected;
                    }
                    if let Some(playback) = snapshot.playback.as_mut() {
                        playback.media_title =
                            resolved.title.clone().or(playback.media_title.take());
                        playback.duration_ms = resolved.duration_ms.or(playback.duration_ms);
                    }
                    snapshot.online = Some(OnlineMediaSnapshot {
                        media_id: resolved.media_id,
                        title: resolved.title,
                        duration_ms: resolved.duration_ms,
                        webpage_url: resolved.webpage_url,
                        extractor: resolved.extractor,
                        chapters: resolved.chapters,
                        subtitles: choices,
                        transcript,
                    });
                }
                Ok(None) => tracing::warn!("online resolve cache unavailable for ACP snapshot"),
                Err(error) => {
                    tracing::warn!(details = ?error.details, "online resolve cache failed")
                }
            }
        }
        if let Some(anchor) = snapshot.anchor.as_ref() {
            tracing::info!(
                position_ms = anchor.position_ms,
                turn = snapshot.session.as_ref().map(|session| session.turn),
                snapshot = %snapshot_path_for_cwd(&session_cwd).display(),
                "ACP 提问锚点已写入 snapshot"
            );
        }
        acp.write_prompt_snapshot(&session_cwd, &snapshot)?;
        acp.prompt(
            text,
            cwd,
            profile_id,
            context,
            history_context,
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
pub async fn acp_set_session_model(
    state: State<'_, AppState>,
    model_id: Option<String>,
    reasoning_effort: Option<String>,
    on_event: Channel<AcpEvent>,
) -> Result<crate::acp::AcpSessionModelOptions, AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || {
        acp.set_session_model(model_id, reasoning_effort, |event| {
            if let Err(error) = on_event.send(event) {
                tracing::warn!(%error, "failed to send ACP set session model event");
            }
        })
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp set session model join: {error}"))))?
}

#[tauri::command]
pub async fn acp_new_chat(
    state: State<'_, AppState>,
    cwd: Option<String>,
    profile_id: Option<String>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        acp.new_chat(cwd, profile_id, settings, profiles, |event| {
            if let Err(error) = on_event.send(event) {
                tracing::warn!(%error, "failed to send ACP new chat event");
            }
        })
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp new chat join: {error}"))))?
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
