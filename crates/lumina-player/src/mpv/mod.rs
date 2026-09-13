//! libmpv implementation. Do not import from React-facing commands.
pub mod events;
pub mod player;

pub use player::{LibMpvPlayer, NetworkPlaybackOpts};
