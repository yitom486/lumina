//! Player state machine, snapshot, and events.

use serde::{Deserialize, Serialize};

use crate::player::error::PlayerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PlayerState {
    Idle,
    Loading,
    Ready,
    Playing,
    Paused,
    Ended,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub status: PlayerState,
    pub current_time_ms: u64,
    pub duration_ms: u64,
    pub volume: f64,
    pub rate: f64,
    pub current_file: Option<String>,
    pub error: Option<PlayerError>,
}

impl PlayerSnapshot {
    pub fn idle() -> Self {
        Self {
            status: PlayerState::Idle,
            current_time_ms: 0,
            duration_ms: 0,
            volume: 100.0,
            rate: 1.0,
            current_file: None,
            error: None,
        }
    }
}

/// Rust → React stream events. Channel wiring is M6.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum PlayerEvent {
    StateChanged { status: PlayerState },
    PositionChanged { position_ms: u64 },
    DurationChanged { duration_ms: u64 },
    FileLoaded { path: String, duration_ms: u64 },
    Ended,
    Error { error: PlayerError },
}
