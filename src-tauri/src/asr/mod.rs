//! Optional on-demand ASR. Not required for playback or subtitle transcripts.

pub mod error;
pub mod extract;
pub mod model;
pub mod paths;
pub mod service;
pub mod whisper_cli;

pub use error::{AsrError, AsrErrorCode};
pub use model::{AsrEvent, AsrModelInfo, AsrRange, AsrStatus};
pub use service::AsrService;
