//! AsrService — optional, on-demand transcription. No model load at startup.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::asr::error::AsrError;
use crate::asr::extract::{self, temp_job_dir};
use crate::asr::model::{AsrEvent, AsrRange, AsrStatus};
use crate::asr::paths::{self, resolve_asr_paths};
use crate::asr::whisper_cli;
use crate::media::MediaInspector;
use crate::subtitle::model::Cue;
use crate::subtitle::Transcript;

pub struct AsrService {
    busy: AtomicBool,
}

#[derive(Debug, Clone)]
struct ResolvedWindow {
    from_ms: u64,
    to_ms: u64,
    /// Sidecar token after `stem.` (`asr`, `asr_ch3`, `asr_part`).
    lang_token: String,
    label: String,
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

    /// Download CLI (if needed) + selected catalog model into app data.
    pub fn install<F>(&self, model_id: &str, mut on_event: F) -> Result<AsrStatus, AsrError>
    where
        F: FnMut(crate::asr::model::AsrInstallEvent),
    {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AsrError::busy());
        }

        let result = crate::asr::download::install_bundle(model_id, |event| {
            on_event(event);
        });

        self.busy.store(false, Ordering::SeqCst);

        if let Err(error) = &result {
            on_event(crate::asr::model::AsrInstallEvent::Failed {
                code: format!("{:?}", error.code),
                message: error.message.clone(),
            });
        }

        result
    }

    /// Runs extract + whisper-cli. Emits progress via callback. Never called implicitly.
    pub fn transcribe<F>(
        &self,
        media_path: impl AsRef<Path>,
        range: Option<AsrRange>,
        model_id: Option<String>,
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

            let asr_paths = resolve_asr_paths(model_id.as_deref())?;
            let window = resolve_window(media_path, range.as_ref())?;

            on_event(AsrEvent::Started {
                path: media_path.to_string_lossy().to_string(),
            });
            on_event(AsrEvent::Progress {
                stage: "extract".into(),
                message: format!("正在抽取音频（{}）…", window.label),
            });

            let work = temp_job_dir(media_path);
            std::fs::create_dir_all(&work).map_err(|error| {
                tracing::warn!(%error, "ASR work dir create failed");
                AsrError::internal(Some(&format!("create ASR work dir: {error}")))
            })?;
            let wav = work.join("audio.wav");
            let extract_range = if window.from_ms == 0 && window.to_ms == u64::MAX {
                None
            } else {
                Some((window.from_ms, window.to_ms))
            };
            extract::extract_wav_16k_mono(media_path, &wav, extract_range)?;

            on_event(AsrEvent::Progress {
                stage: "transcribe".into(),
                message: format!("正在转写语音（{}）…", window.label),
            });

            let mut transcript = whisper_cli::transcribe_wav(&asr_paths, &wav, &work, media_path)?;
            offset_cues(&mut transcript.cues, window.from_ms);
            reindex_cues(&mut transcript.cues);

            on_event(AsrEvent::Progress {
                stage: "export".into(),
                message: "正在保存外挂字幕…".into(),
            });
            let transcript = crate::subtitle::write::export_sidecar_srt(
                media_path,
                &window.lang_token,
                &transcript.cues,
            )
            .map_err(|error| {
                tracing::warn!(%error, "ASR sidecar export failed");
                AsrError::export_failed(error.details.as_deref())
            })?;

            on_event(AsrEvent::Finished {
                transcript: transcript.clone(),
            });

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

fn resolve_window(media_path: &Path, range: Option<&AsrRange>) -> Result<ResolvedWindow, AsrError> {
    let Some(range) = range else {
        return Ok(ResolvedWindow {
            from_ms: 0,
            to_ms: u64::MAX,
            lang_token: "asr".into(),
            label: "整片".into(),
        });
    };

    match range {
        AsrRange::Window { from_ms, to_ms } => {
            if *to_ms <= *from_ms {
                return Err(AsrError::invalid("转写时间范围无效"));
            }
            Ok(ResolvedWindow {
                from_ms: *from_ms,
                to_ms: *to_ms,
                lang_token: "asr_part".into(),
                label: "指定区间".into(),
            })
        }
        AsrRange::Chapter { chapter_id } => {
            let info = MediaInspector::inspect(media_path).map_err(|error| {
                tracing::warn!(%error, "ASR chapter resolve failed");
                AsrError::extract_failed(error.details.as_deref())
            })?;
            let chapter = info
                .chapters
                .iter()
                .find(|c| c.id == *chapter_id)
                .ok_or_else(|| AsrError::invalid("找不到该章节"))?;
            let to_ms = chapter
                .end_ms
                .or(info.duration_ms)
                .ok_or_else(|| AsrError::invalid("该章节缺少结束时间"))?;
            if to_ms <= chapter.start_ms {
                return Err(AsrError::invalid("转写时间范围无效"));
            }
            let title = chapter
                .title
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("当前章节");
            Ok(ResolvedWindow {
                from_ms: chapter.start_ms,
                to_ms,
                lang_token: format!("asr_ch{chapter_id}"),
                label: title.to_string(),
            })
        }
    }
}

fn offset_cues(cues: &mut [Cue], offset_ms: u64) {
    if offset_ms == 0 {
        return;
    }
    for cue in cues.iter_mut() {
        cue.start_ms = cue.start_ms.saturating_add(offset_ms);
        cue.end_ms = cue.end_ms.saturating_add(offset_ms);
    }
}

fn reindex_cues(cues: &mut [Cue]) {
    for (i, cue) in cues.iter_mut().enumerate() {
        cue.index = (i + 1) as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_cues_shifts_times() {
        let mut cues = vec![Cue {
            index: 1,
            start_ms: 100,
            end_ms: 200,
            text: "a".into(),
        }];
        offset_cues(&mut cues, 60_000);
        assert_eq!(cues[0].start_ms, 60_100);
        assert_eq!(cues[0].end_ms, 60_200);
    }

    #[test]
    fn window_range_rejects_inverted() {
        let err = resolve_window(
            Path::new("missing.mkv"),
            Some(&AsrRange::Window {
                from_ms: 10,
                to_ms: 5,
            }),
        )
        .expect_err("inverted");
        assert_eq!(err.code, crate::asr::AsrErrorCode::InvalidRequest);
        assert!(err.message.contains("时间范围"));
    }
}
