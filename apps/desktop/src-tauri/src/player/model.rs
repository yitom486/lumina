//! Player state machine, snapshot, and events.

use serde::{Deserialize, Serialize};

use crate::player::error::PlayerError;
use crate::player::source::MediaSourceKind;

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
    /// Playback target shown to UI (local path or original URL).
    pub current_file: Option<String>,
    /// Stable id for notes / history (`path` or `youtube:…` / `bilibili:…`).
    #[serde(default)]
    pub media_id: Option<String>,
    #[serde(default)]
    pub source_kind: Option<MediaSourceKind>,
    /// Active online format id when `source_kind` is remote; local stays `None`.
    #[serde(default)]
    pub playback_format_id: Option<String>,
    /// yt-dlp duration for UI only — not proof that mpv demux succeeded.
    #[serde(default)]
    pub duration_hint_ms: Option<u64>,
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
            media_id: None,
            source_kind: None,
            playback_format_id: None,
            duration_hint_ms: None,
            error: None,
        }
    }
}

/// Rust → React stream events (Tauri Channel).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "PascalCase",
    rename_all_fields = "camelCase"
)]
pub enum PlayerEvent {
    StateChanged {
        status: PlayerState,
    },
    PositionChanged {
        position_ms: u64,
    },
    DurationChanged {
        duration_ms: u64,
    },
    FileLoaded {
        path: String,
        duration_ms: u64,
    },
    Ended,
    Error {
        error: PlayerError,
    },
    /// Left-click on the native video surface (after click/dblclick discrimination on UI).
    SurfaceClick,
    /// Double-click on the native video surface.
    SurfaceDoubleClick,
}
