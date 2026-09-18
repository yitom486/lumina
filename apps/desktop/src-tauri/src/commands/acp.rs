//! On-demand ACP commands. Never runs unless the frontend invokes them.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::acp::adapter;
use crate::acp::{
    AcpError, AcpEvent, AcpStatus, AgentProfilesHint, AgentSessionListResult, PromptImage,
    SavedSessionHint, VideoPromptContext,
};
use crate::mcp::OnlineMediaSnapshot;
use crate::state::AppState;
use lumina_acp::agent::workspace::resolve_session_cwd;
use lumina_acp::AcpClientSettings;

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
pub async fn acp_list_agent_sessions(
    state: State<'_, AppState>,
    cwd: Option<String>,
) -> Result<AgentSessionListResult, AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || acp.list_agent_sessions(cwd.as_deref()))
        .await
        .map_err(|error| AcpError::internal(Some(&format!("acp list sessions join: {error}"))))?
}

#[tauri::command]
pub async fn acp_load_session(
    state: State<'_, AppState>,
    session_id: String,
    cwd: Option<String>,
) -> Result<Vec<lumina_acp::runtime::service::LoadedTurn>, AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || acp.load_session_transcript(session_id, cwd))
        .await
        .map_err(|error| AcpError::internal(Some(&format!("acp load session join: {error}"))))?
}

#[tauri::command]
pub async fn acp_delete_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    tauri::async_runtime::spawn_blocking(move || acp.delete_session(session_id))
        .await
        .map_err(|error| AcpError::internal(Some(&format!("acp delete session join: {error}"))))?
}

#[tauri::command]
pub async fn acp_sync_mcp_capabilities(
    _state: State<'_, AppState>,
    cwd: Option<String>,
    client_settings: Option<AcpClientSettings>,
) -> Result<(), AcpError> {
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        adapter::sync_mcp_capabilities(cwd.as_deref(), settings.vision_capable)
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
    images: Option<Vec<PromptImage>>,
    saved_session: Option<SavedSessionHint>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<String, AcpError> {
    let acp = state.acp.clone();
    let library = state.library.clone();
    let snapshots = state.prompt_snapshots.clone();
    let ytdl = state.ytdl.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        let session_cwd = resolve_session_cwd(cwd.as_deref())?;
        let (mut snapshot, media_changed) = adapter::build_prompt_snapshot(
            &snapshots,
            &library,
            context.as_ref(),
            settings.vision_capable,
        )?;
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
                                    "online transcript unavailable for ACP snapshot"
                                );
                                None
                            }
                        }
                    });
                    if let Some(anchor) = snapshot.anchor.as_mut() {
                        anchor.subtitle_choice_id = selected;
                        if anchor
                            .media_title
                            .as_ref()
                            .is_none_or(|s| s.trim().is_empty())
                        {
                            anchor.media_title = resolved.title.clone();
                        }
                        if anchor.duration_ms.is_none() {
                            anchor.duration_ms = resolved.duration_ms;
                        }
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
                    tracing::warn!(code = ?error.code, "online resolve cache failed")
                }
            }
        }
        if let Some(anchor) = snapshot.anchor.as_ref() {
            tracing::info!(
                position_ms = anchor.position_ms,
                media_changed,
                turn = snapshot.session.as_ref().map(|session| session.turn),
                "ACP 提问锚点已写入 snapshot"
            );
        }
        adapter::write_prompt_snapshot(&session_cwd, &snapshot)?;
        let context = enrich_prompt_context(context, &snapshot, media_changed);
        let images = images.unwrap_or_default();
        acp.prompt(
            text,
            cwd,
            profile_id,
            context,
            images,
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

/// Per-turn: progress always. Episode plot only when media/episode switched.
fn enrich_prompt_context(
    context: Option<VideoPromptContext>,
    snapshot: &lumina_mcp::LuminaMcpSnapshot,
    media_changed: bool,
) -> Option<VideoPromptContext> {
    let mut ctx = context.unwrap_or_default();
    ctx.episode_title = None;
    ctx.episode_overview = None;

    if let Some(anchor) = snapshot.anchor.as_ref() {
        if ctx.media_path.as_ref().is_none_or(|s| s.trim().is_empty()) {
            ctx.media_path = Some(anchor.media_path.clone());
        }
        if ctx.media_title.as_ref().is_none_or(|s| s.trim().is_empty()) {
            ctx.media_title = anchor.media_title.clone();
        }
        if ctx.position_ms.is_none() {
            ctx.position_ms = Some(anchor.position_ms);
        }
        if ctx.duration_ms.is_none() {
            ctx.duration_ms = anchor.duration_ms;
        }
        if ctx
            .subtitle_choice_id
            .as_ref()
            .is_none_or(|s| s.trim().is_empty())
        {
            ctx.subtitle_choice_id = anchor.subtitle_choice_id.clone();
        }
        if ctx.season.is_none() {
            ctx.season = anchor.season;
        }
        if ctx.episode.is_none() {
            ctx.episode = anchor.episode;
        }
    }
    if let Some(episode) = snapshot.current_episode.as_ref() {
        if ctx.season.is_none() {
            ctx.season = episode.season;
        }
        if ctx.episode.is_none() {
            ctx.episode = episode.episode;
        }
        // Conditional: pack episode plot into the prompt only on media switch.
        if media_changed {
            ctx.episode_title = episode.title.clone();
            ctx.episode_overview = episode.overview.clone();
        }
    }
    if ctx.is_empty() {
        None
    } else {
        Some(ctx)
    }
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
    let snapshots = state.prompt_snapshots.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        adapter::reset_prompt_snapshot_state(&snapshots);
        acp.new_chat(cwd, profile_id, settings, profiles, |event| {
            if let Err(error) = on_event.send(event) {
                tracing::warn!(%error, "failed to send ACP new chat event");
            }
        })
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp new chat join: {error}"))))?
}

/// Switch the visible conversation on the live agent process when possible
/// (close + resume/new without respawning). Falls back to a full connect
/// honoring the hint when no live process exists.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn acp_switch_session(
    state: State<'_, AppState>,
    cwd: Option<String>,
    profile_id: Option<String>,
    saved_session: Option<SavedSessionHint>,
    client_settings: Option<AcpClientSettings>,
    profiles: AgentProfilesHint,
    on_event: Channel<AcpEvent>,
) -> Result<(), AcpError> {
    let acp = state.acp.clone();
    let snapshots = state.prompt_snapshots.clone();
    let settings = client_settings.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        adapter::reset_prompt_snapshot_state(&snapshots);
        acp.switch_session(
            cwd,
            profile_id,
            saved_session,
            settings,
            profiles,
            |event| {
                if let Err(error) = on_event.send(event) {
                    tracing::warn!(%error, "failed to send ACP switch session event");
                }
            },
        )
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp switch session join: {error}"))))?
}

#[tauri::command]
pub async fn acp_close(app: AppHandle) -> Result<(), AcpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(AcpError::internal(Some("app state unavailable")));
        };
        let result = state.acp.close_session();
        adapter::reset_prompt_snapshot_state(&state.prompt_snapshots);
        result
    })
    .await
    .map_err(|error| AcpError::internal(Some(&format!("acp close join: {error}"))))?
}

#[tauri::command]
pub async fn acp_login_antigravity(proxy_port: Option<u16>) -> Result<String, AcpError> {
    tauri::async_runtime::spawn_blocking(move || lumina_acp::login_antigravity(proxy_port))
        .await
        .map_err(|error| {
            AcpError::internal(Some(&format!("acp login antigravity join: {error}")))
        })?
}
