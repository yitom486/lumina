//! Subtitle / transcript domain (library crate).
//!
//! Agent-backed translation stays in the app (`translate` bridge) until M5 `lumina-ai`.

pub mod error;
pub mod extract;
pub mod model;
pub mod parse;
pub mod service;
pub mod write;

pub use error::{SubtitleError, SubtitleErrorCode};
pub use model::{Cue, SubtitleChoice, SubtitleSource, Transcript};
pub use service::SubtitleService;
