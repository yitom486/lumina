//! App compat: player implementation lives in `lumina-player`
//! (player-crate migration).
//!
//! Existing `crate::player::…` paths keep working through this re-export.

pub use lumina_player::{
    error, model, service, source, MediaSource, MediaSourceKind, PlayerError, PlayerErrorCode,
    PlayerEvent, PlayerService, PlayerSnapshot, PlayerState,
};

/// Tauri/native-bound mpv glue stays in app (L2 player-crate audit §5).
pub mod mpv;
