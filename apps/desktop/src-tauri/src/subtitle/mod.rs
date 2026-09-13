//! Subtitle / transcript domain.

pub mod error;
pub mod extract;
pub mod model;
pub mod parse;
pub mod service;
pub mod translate;
pub mod write;

pub use error::{SubtitleError, SubtitleErrorCode};
pub use model::{Cue, SubtitleChoice, SubtitleSource, Transcript};
pub use service::SubtitleService;
