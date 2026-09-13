//! Optional on-demand ASR (library crate). Not required for playback or subtitle transcripts.

pub mod download;
pub mod error;
pub mod extract;
pub mod model;
pub mod paths;
pub mod service;
pub mod transcriber;
pub mod whisper_cli;

pub use error::{AsrError, AsrErrorCode};
pub use model::{AsrCatalogModel, AsrEvent, AsrInstallEvent, AsrModelInfo, AsrRange, AsrStatus};
pub use service::AsrService;
pub use transcriber::{Transcriber, WhisperCliTranscriber};
