//! Subtitle / transcript domain.

pub mod error;
pub mod extract;
pub mod model;
pub mod parse;
pub mod service;

pub use error::{SubtitleError, SubtitleErrorCode};
pub use model::{Cue, SubtitleTrackInfo, Transcript};
pub use service::SubtitleService;
