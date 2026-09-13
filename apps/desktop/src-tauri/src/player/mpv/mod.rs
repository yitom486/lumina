//! App-side mpv glue: portable core is re-exported from `lumina-player`;
//! Tauri-bound `native_library` / `window` and the manual `baseline` stay here
//! (L2 player-crate audit §5).

pub use lumina_player::mpv::{LibMpvPlayer, NetworkPlaybackOpts};

#[cfg(test)]
mod baseline;
pub mod native_library;
pub mod window;
