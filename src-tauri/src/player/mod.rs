//! Player domain API. libmpv types stay in `mpv`.

pub mod error;
pub mod model;
pub mod mpv;
pub mod service;

pub use error::{PlayerError, PlayerErrorCode};
pub use model::{PlayerEvent, PlayerSnapshot, PlayerState};
pub use service::PlayerService;
