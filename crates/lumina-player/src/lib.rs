//! Player domain: PlayerService, libmpv playback, playback state machine.
//!
//! Depends only on [`lumina_core`] (plus serde/tracing/libmpv2). Online
//! playback targets, Cookie files, and yt-dlp runtime paths are assembled by
//! `lumina-app` and passed in via [`mpv::NetworkPlaybackOpts`]; Tauri windows,
//! native surfaces, resource lookup, and commands stay in the app.

pub mod error;
pub mod model;
pub mod mpv;
pub mod service;
pub mod source;

pub use error::{PlayerError, PlayerErrorCode};
pub use model::{PlayerEvent, PlayerSnapshot, PlayerState};
pub use service::PlayerService;
pub use source::{MediaSource, MediaSourceKind};
