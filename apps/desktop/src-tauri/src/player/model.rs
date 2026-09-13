//! Player state machine, snapshot, and events.

use serde::{Deserialize, Serialize};

use crate::player::error::PlayerError;
use lumina_core::MediaSourceKind;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_snapshot_keeps_page_url_without_sensitive_fields() {
        let snapshot = PlayerSnapshot {
            status: PlayerState::Paused,
            current_time_ms: 10_000,
            duration_ms: 0,
            volume: 100.0,
            rate: 1.0,
            current_file: Some("https://www.youtube.com/watch?v=abc".into()),
            media_id: Some("youtube:abc".into()),
            source_kind: Some(MediaSourceKind::Remote),
            playback_format_id: Some("22".into()),
            duration_hint_ms: Some(60_000),
            error: None,
        };
        let text = serde_json::to_value(&snapshot)
            .expect("serialize")
            .to_string()
            .to_lowercase();
        // Functional identity survives.
        assert!(text.contains("watch?v=abc"), "page url: {text}");
        assert!(text.contains("youtube:abc"), "media id: {text}");
        // Backend-only network material never crosses into the UI snapshot.
        for banned in [
            "cookie",
            "sig=",
            "signed",
            "ytdl_cli",
            "cookies-file",
            "yt-dlp.exe",
            "stderr",
            "--cookies",
        ] {
            assert!(!text.contains(banned), "banned {banned}: {text}");
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
