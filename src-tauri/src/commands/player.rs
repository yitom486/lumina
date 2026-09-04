//! Player Tauri commands. React talks to PlayerService only.
//!
//! Sync commands run on the UI/main thread in Tauri 2 — anything that may
//! block (mpv loadfile, etc.) must use async + spawn_blocking.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::player::error::PlayerError;
use crate::player::model::{PlayerEvent, PlayerSnapshot, PlayerState};
use crate::player::source::{MediaSource, MediaSourceKind};
use crate::state::AppState;
use crate::ytdl::error::YtdlError;
use crate::ytdl::PlaybackFormatsResponse;

async fn on_worker<R, F>(app: AppHandle, work: F) -> Result<R, PlayerError>
where
    R: Send + 'static,
    F: FnOnce(&AppState) -> Result<R, PlayerError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(PlayerError::internal(Some("app state unavailable")));
        };
        work(state.inner())
    })
    .await
    .map_err(|error| PlayerError::internal(Some(&format!("player task join: {error}"))))?
}

fn ytdl_to_player(error: YtdlError) -> PlayerError {
    PlayerError::new(
        crate::player::PlayerErrorCode::LoadError,
        error.message,
        error.details,
    )
}

fn emit_open_error(state: &AppState, error: PlayerError) -> PlayerError {
    state.emit(PlayerEvent::Error {
        error: error.clone(),
    });
    state.emit(PlayerEvent::StateChanged {
        status: PlayerState::Error,
    });
    error
}

#[tauri::command]
pub fn player_subscribe(
    app: AppHandle,
    state: State<'_, AppState>,
    on_event: Channel<PlayerEvent>,
) -> Result<(), PlayerError> {
    state.set_event_channel(on_event)?;
    state.ensure_event_ticker(app);
    Ok(())
}

#[tauri::command]
pub async fn player_get_state(app: AppHandle) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, |state| {
        state.with_player(|player| Ok(player.snapshot()))
    })
    .await
}

#[tauri::command]
pub async fn player_open(app: AppHandle, path: String) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, move |state| open_media(state, path)).await
}

fn open_media(state: &AppState, path: String) -> Result<PlayerSnapshot, PlayerError> {
    let source = match MediaSource::parse(&path) {
        Ok(source) => source,
        Err(error) => return Err(emit_open_error(state, error)),
    };

    match source.kind() {
        MediaSourceKind::Local => match state.with_player(|player| player.open(path)) {
            Ok((snapshot, events)) => {
                state.emit_all(events);
                Ok(snapshot)
            }
            Err(error) => Err(emit_open_error(state, error)),
        },
        MediaSourceKind::Remote => open_remote(state, source, None),
    }
}

fn open_remote(
    state: &AppState,
    source: MediaSource,
    format_id: Option<String>,
) -> Result<PlayerSnapshot, PlayerError> {
    let page_url = source.playback_target().to_string();
    let target = state
        .ytdl()
        .play_target(&page_url, format_id.as_deref())
        .map_err(|error| emit_open_error(state, ytdl_to_player(error)))?;

    let open_result = state.with_player(|player| {
        player.open_source(
            source,
            Some(target.stream_url),
            Some(target.format_id),
            target.audio_url,
            None,
            false,
        )
    });

    match open_result {
        Ok((snapshot, events)) => {
            state.emit_all(events);
            Ok(snapshot)
        }
        Err(error) => Err(emit_open_error(state, error)),
    }
}

#[tauri::command]
pub async fn player_list_playback_formats(
    app: AppHandle,
) -> Result<PlaybackFormatsResponse, PlayerError> {
    on_worker(app, |state| {
        state.ytdl().list_formats().map_err(ytdl_to_player)
    })
    .await
}

#[tauri::command]
pub async fn player_set_playback_format(
    app: AppHandle,
    format_id: String,
) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, move |state| set_playback_format(state, format_id)).await
}

fn set_playback_format(state: &AppState, format_id: String) -> Result<PlayerSnapshot, PlayerError> {
    let format_id = format_id.trim().to_string();
    if format_id.is_empty() {
        return Err(PlayerError::playback(Some("empty format id")));
    }

    let snap = state.with_player(|player| Ok(player.snapshot()))?;
    if snap.source_kind != Some(MediaSourceKind::Remote) {
        return Err(PlayerError::playback(Some(
            "format switch requires remote source",
        )));
    }
    if snap.playback_format_id.as_deref() == Some(format_id.as_str()) {
        return Ok(snap);
    }

    let page_url = snap
        .current_file
        .clone()
        .ok_or_else(|| PlayerError::playback(Some("missing remote page url")))?;

    let position_ms = snap.current_time_ms;
    let rate = snap.rate;
    let was_paused = snap.status == PlayerState::Paused;

    let target = state
        .ytdl()
        .play_target_cached(&page_url, &format_id)
        .map_err(|error| {
            PlayerError::new(
                crate::player::PlayerErrorCode::PlaybackError,
                "切换清晰度失败，请重试",
                error.details.or(Some(error.message)),
            )
        })?;

    let source = MediaSource::parse(&target.page_url)?;
    let open_result = state.with_player(|player| {
        player.open_source(
            source,
            Some(target.stream_url),
            Some(target.format_id),
            target.audio_url,
            Some(position_ms).filter(|ms| *ms > 0),
            was_paused,
        )
    });

    let (mut snapshot, events) = match open_result {
        Ok(ok) => ok,
        Err(error) => {
            return Err(PlayerError::new(
                crate::player::PlayerErrorCode::PlaybackError,
                "切换清晰度失败，请重试",
                error.details.or(Some(error.message)),
            ));
        }
    };
    state.emit_all(events);

    // open_source restores snapshot.rate; re-assert in case backend dropped it.
    if let Ok(snap) = state.with_player(|player| player.set_rate(rate)) {
        snapshot = snap;
    }

    Ok(snapshot)
}

#[tauri::command]
pub async fn player_play(app: AppHandle) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, |state| {
        let (snapshot, events) = state.with_player(|player| player.play())?;
        state.emit_all(events);
        Ok(snapshot)
    })
    .await
}

#[tauri::command]
pub async fn player_pause(app: AppHandle) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, |state| {
        let (snapshot, events) = state.with_player(|player| player.pause())?;
        state.emit_all(events);
        Ok(snapshot)
    })
    .await
}

#[tauri::command]
pub async fn player_stop(app: AppHandle) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, |state| {
        let (snapshot, events) = state.with_player(|player| player.stop())?;
        state.emit_all(events);
        Ok(snapshot)
    })
    .await
}

#[tauri::command]
pub async fn player_seek(app: AppHandle, position_ms: u64) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, move |state| {
        let (snapshot, events) = state.with_player(|player| player.seek(position_ms))?;
        state.emit_all(events);
        Ok(snapshot)
    })
    .await
}

#[tauri::command]
pub async fn player_set_volume(app: AppHandle, volume: f64) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, move |state| {
        state.with_player(|player| player.set_volume(volume))
    })
    .await
}

#[tauri::command]
pub async fn player_set_rate(app: AppHandle, rate: f64) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, move |state| {
        state.with_player(|player| player.set_rate(rate))
    })
    .await
}

#[tauri::command]
pub async fn player_set_subtitle(
    app: AppHandle,
    source: String,
    stream_index: Option<u32>,
    external_path: Option<String>,
) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, move |state| {
        state.with_player(|player| {
            player.set_subtitle(&source, stream_index, external_path.as_deref())
        })
    })
    .await
}

#[tauri::command]
pub async fn player_set_audio(
    app: AppHandle,
    stream_index: u32,
) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, move |state| {
        state.with_player(|player| player.set_audio_track(stream_index))
    })
    .await
}

/// HWND layout must stay on the UI thread (Win32 parenting).
#[tauri::command]
pub fn player_set_surface_bounds(
    state: State<'_, AppState>,
    window: WebviewWindow,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), PlayerError> {
    let scale = window.scale_factor().map_err(|error| {
        PlayerError::new(
            crate::player::PlayerErrorCode::InternalError,
            "无法读取窗口缩放比例",
            Some(error.to_string()),
        )
    })?;

    let to_px = |value: f64| (value * scale).round() as i32;
    state
        .with_surface(|surface| surface.set_bounds(to_px(x), to_px(y), to_px(width), to_px(height)))
}
