//! Player Tauri commands. React talks to PlayerService only.

use tauri::{State, WebviewWindow};

use crate::player::error::PlayerError;
use crate::player::model::PlayerSnapshot;
use crate::state::AppState;

#[tauri::command]
pub fn player_get_state(state: State<'_, AppState>) -> Result<PlayerSnapshot, PlayerError> {
    state.with_player(|player| Ok(player.snapshot()))
}

#[tauri::command]
pub fn player_open(state: State<'_, AppState>, path: String) -> Result<PlayerSnapshot, PlayerError> {
    state.with_player(|player| player.open(path))
}

#[tauri::command]
pub fn player_play(state: State<'_, AppState>) -> Result<PlayerSnapshot, PlayerError> {
    state.with_player(|player| player.play())
}

#[tauri::command]
pub fn player_pause(state: State<'_, AppState>) -> Result<PlayerSnapshot, PlayerError> {
    state.with_player(|player| player.pause())
}

#[tauri::command]
pub fn player_stop(state: State<'_, AppState>) -> Result<PlayerSnapshot, PlayerError> {
    state.with_player(|player| player.stop())
}

#[tauri::command]
pub fn player_seek(
    state: State<'_, AppState>,
    position_ms: u64,
) -> Result<PlayerSnapshot, PlayerError> {
    state.with_player(|player| player.seek(position_ms))
}

#[tauri::command]
pub fn player_set_volume(
    state: State<'_, AppState>,
    volume: f64,
) -> Result<PlayerSnapshot, PlayerError> {
    state.with_player(|player| player.set_volume(volume))
}

#[tauri::command]
pub fn player_set_rate(
    state: State<'_, AppState>,
    rate: f64,
) -> Result<PlayerSnapshot, PlayerError> {
    state.with_player(|player| player.set_rate(rate))
}

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
            "failed to read window scale factor",
            Some(error.to_string()),
        )
    })?;

    let to_px = |value: f64| (value * scale).round() as i32;
    state.with_surface(|surface| {
        surface.set_bounds(to_px(x), to_px(y), to_px(width), to_px(height))
    })
}
