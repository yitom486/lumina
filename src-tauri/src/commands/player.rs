//! Player Tauri commands. React talks to PlayerService only.
//!
//! Sync commands run on the UI/main thread in Tauri 2 — anything that may
//! block (mpv loadfile, etc.) must use async + spawn_blocking.

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::player::error::PlayerError;
use crate::player::model::{PlayerEvent, PlayerSnapshot};
use crate::state::AppState;

async fn on_worker<R, F>(app: AppHandle, work: F) -> Result<R, PlayerError>
where
    R: Send + 'static,
    F: FnOnce(&AppState) -> Result<R, PlayerError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return Err(PlayerError::internal("应用状态不可用", None));
        };
        work(state.inner())
    })
    .await
    .map_err(|error| {
        PlayerError::internal("播放任务异常结束", Some(&error.to_string()))
    })?
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
    on_worker(app, |state| state.with_player(|player| Ok(player.snapshot()))).await
}

#[tauri::command]
pub async fn player_open(app: AppHandle, path: String) -> Result<PlayerSnapshot, PlayerError> {
    on_worker(app, move |state| match state.with_player(|player| player.open(path)) {
        Ok((snapshot, events)) => {
            state.emit_all(events);
            Ok(snapshot)
        }
        Err(error) => {
            state.emit(PlayerEvent::Error {
                error: error.clone(),
            });
            state.emit(PlayerEvent::StateChanged {
                status: crate::player::PlayerState::Error,
            });
            Err(error)
        }
    })
    .await
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
    on_worker(app, move |state| state.with_player(|player| player.set_rate(rate))).await
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
    state.with_surface(|surface| {
        surface.set_bounds(to_px(x), to_px(y), to_px(width), to_px(height))
    })
}
