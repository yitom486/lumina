//! Stable transcription boundary for the ASR pipeline.
//!
//! `AsrService` always runs the bundled whisper-cli through this trait.
//! Future engines (or an AI-polished pipeline in `lumina-ai`) implement the
//! same interface; install / status / window / sidecar / error behavior is
//! owned by `AsrService` and never changes with the engine.

use std::path::Path;

use lumina_subtitle::Transcript;

use crate::error::AsrError;
use crate::paths::AsrPaths;

pub trait Transcriber {
    fn transcribe_wav(
        &self,
        paths: &AsrPaths,
        wav_path: &Path,
        work_dir: &Path,
        media_path: &Path,
    ) -> Result<Transcript, AsrError>;
}

/// Default on-demand engine: spawn the project-local `whisper-cli`.
pub struct WhisperCliTranscriber;

impl Transcriber for WhisperCliTranscriber {
    fn transcribe_wav(
        &self,
        paths: &AsrPaths,
        wav_path: &Path,
        work_dir: &Path,
        media_path: &Path,
    ) -> Result<Transcript, AsrError> {
        crate::whisper_cli::transcribe_wav(paths, wav_path, work_dir, media_path)
    }
}
