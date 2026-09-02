//! AsrService — optional, on-demand transcription. No model load at startup.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::asr::error::AsrError;
use crate::asr::extract::{self, temp_job_dir};
use crate::asr::model::{AsrEvent, AsrStatus};
use crate::asr::paths::{self, resolve_asr_paths};
use crate::asr::whisper_cli;
use crate::subtitle::Transcript;

pub struct AsrService {
    busy: AtomicBool,
}

impl AsrService {
    pub fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
        }
    }

    pub fn status(&self) -> AsrStatus {
        paths::status()
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// Runs extract + whisper-cli. Emits progress via callback. Never called implicitly.
    pub fn transcribe<F>(
        &self,
        media_path: impl AsRef<Path>,
        mut on_event: F,
    ) -> Result<Transcript, AsrError>
    where
        F: FnMut(AsrEvent),
    {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AsrError::busy());
        }

        let result = (|| {
            let media_path = media_path.as_ref();
            if !media_path.is_file() {
                return Err(AsrError::extract_failed(Some(&format!(
                    "media file not found: {}",
                    media_path.to_string_lossy()
                ))));
            }

            let asr_paths = resolve_asr_paths()?;
            on_event(AsrEvent::Started {
                path: media_path.to_string_lossy().to_string(),
            });
            on_event(AsrEvent::Progress {
                stage: "extract".into(),
                message: "正在抽取音频…".into(),
            });

            let work = temp_job_dir(media_path);
            std::fs::create_dir_all(&work).map_err(|error| {
                tracing::warn!(%error, "ASR work dir create failed");
                AsrError::internal(Some(&format!("create ASR work dir: {error}")))
            })?;
            let wav = work.join("audio.wav");
            extract::extract_wav_16k_mono(media_path, &wav)?;

            on_event(AsrEvent::Progress {
                stage: "transcribe".into(),
                message: "正在转写语音…".into(),
            });

            let transcript = whisper_cli::transcribe_wav(&asr_paths, &wav, &work, media_path)?;

            on_event(AsrEvent::Progress {
                stage: "export".into(),
                message: "正在保存外挂字幕…".into(),
            });
            let transcript = crate::subtitle::write::export_asr_sidecar(media_path, &transcript)
                .map_err(|error| {
                    tracing::warn!(%error, "ASR sidecar export failed");
                    AsrError::export_failed(error.details.as_deref())
                })?;

            on_event(AsrEvent::Finished {
                transcript: transcript.clone(),
            });

            // Best-effort cleanup of large wav; keep srt for debugging is optional — remove both.
            let _ = std::fs::remove_file(&wav);
            Ok(transcript)
        })();

        self.busy.store(false, Ordering::SeqCst);

        if let Err(error) = &result {
            on_event(AsrEvent::Failed {
                code: format!("{:?}", error.code),
                message: error.message.clone(),
            });
        }

        result
    }
}

impl Default for AsrService {
    fn default() -> Self {
        Self::new()
    }
}
