//! SubtitleService — list choices (embedded + sidecar) and load transcripts.

use std::path::{Path, PathBuf};

use crate::media::{MediaInspector, StreamKind};
use crate::subtitle::error::SubtitleError;
use crate::subtitle::extract::{self, is_bitmap_codec};
use crate::subtitle::model::{SubtitleChoice, SubtitleSource, Transcript};
use crate::subtitle::parse::parse_subtitle_text;

const SIDECAR_EXTS: &[&str] = &["srt", "ass", "ssa", "vtt"];

pub struct SubtitleService;

impl SubtitleService {
    /// Embedded subtitle streams + sidecar files next to the video (same stem / stem.*).
    pub fn list_choices(path: impl AsRef<Path>) -> Result<Vec<SubtitleChoice>, SubtitleError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(SubtitleError::file_not_found(&path.to_string_lossy()));
        }

        let mut choices = Vec::new();

        match MediaInspector::inspect(path) {
            Ok(info) => {
                for stream in info.streams.iter().filter(|s| s.kind == StreamKind::Subtitle) {
                    let supported = !is_bitmap_codec(stream.codec_name.as_deref());
                    let lang = stream.language.as_deref().unwrap_or("und");
                    let codec = stream.codec_name.as_deref().unwrap_or("sub");
                    let label = if supported {
                        format!("内嵌 · {lang} · {codec}")
                    } else {
                        format!("内嵌 · {lang} · {codec}（位图不可用）")
                    };
                    choices.push(SubtitleChoice {
                        id: format!("embedded:{}", stream.index),
                        source: SubtitleSource::Embedded,
                        label,
                        supported,
                        stream_index: Some(stream.index),
                        external_path: None,
                        codec_name: stream.codec_name.clone(),
                        language: stream.language.clone(),
                    });
                }
            }
            Err(error) => {
                tracing::warn!(%error, "media inspect failed while listing subtitle choices");
            }
        }

        for sidecar in discover_sidecars(path) {
            let ext = sidecar
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("sub")
                .to_ascii_lowercase();
            let name = sidecar
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("subtitle");
            let lang_hint = language_hint_from_sidecar_name(path, &sidecar);
            let label = match &lang_hint {
                Some(lang) => format!("外挂 · {lang} · {name}"),
                None => format!("外挂 · {name}"),
            };
            choices.push(SubtitleChoice {
                id: format!("sidecar:{}", sidecar.to_string_lossy()),
                source: SubtitleSource::Sidecar,
                label,
                supported: true,
                stream_index: None,
                external_path: Some(sidecar.to_string_lossy().to_string()),
                codec_name: Some(ext),
                language: lang_hint,
            });
        }

        Ok(choices)
    }

    pub fn load_choice(
        media_path: impl AsRef<Path>,
        choice_id: impl AsRef<str>,
    ) -> Result<Transcript, SubtitleError> {
        let media_path = media_path.as_ref();
        let choice_id = choice_id.as_ref();
        let choices = Self::list_choices(media_path)?;
        let choice = choices
            .iter()
            .find(|c| c.id == choice_id)
            .ok_or_else(|| {
                SubtitleError::extract_failed(
                    "subtitle choice not found",
                    Some(choice_id),
                )
            })?;

        if !choice.supported {
            return Err(SubtitleError::unsupported(
                "bitmap subtitles are not supported (no OCR yet)",
                choice.codec_name.as_deref(),
            ));
        }

        match choice.source {
            SubtitleSource::Embedded => {
                let stream_index = choice.stream_index.ok_or_else(|| {
                    SubtitleError::internal("embedded choice missing stream index", None)
                })?;
                let (content, _format) = extract::extract_text_subtitle(
                    media_path,
                    stream_index,
                    choice.codec_name.as_deref(),
                )?;
                let cues = parse_subtitle_text(&content)?;
                Ok(Transcript {
                    source_path: media_path.to_string_lossy().to_string(),
                    choice_id: choice.id.clone(),
                    stream_index: Some(stream_index),
                    language: choice.language.clone(),
                    codec_name: choice.codec_name.clone(),
                    cues,
                })
            }
            SubtitleSource::Sidecar => {
                let external = choice.external_path.as_deref().ok_or_else(|| {
                    SubtitleError::internal("sidecar choice missing path", None)
                })?;
                let (content, path_buf) = extract::read_external_subtitle(Path::new(external))?;
                let cues = parse_subtitle_text(&content)?;
                Ok(Transcript {
                    source_path: path_buf.to_string_lossy().to_string(),
                    choice_id: choice.id.clone(),
                    stream_index: None,
                    language: choice.language.clone(),
                    codec_name: choice.codec_name.clone(),
                    cues,
                })
            }
        }
    }
}

fn discover_sidecars(media_path: &Path) -> Vec<PathBuf> {
    let Some(parent) = media_path.parent() else {
        return Vec::new();
    };
    let Some(stem) = media_path.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let stem_lower = stem.to_ascii_lowercase();

    let mut found = Vec::new();
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(_) => return found,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !SIDECAR_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
            continue;
        }
        let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let file_stem_lower = file_stem.to_ascii_lowercase();
        // Exact: video.srt
        // Prefixed: video.en.srt / video.zh-CN.ass
        if file_stem_lower == stem_lower
            || file_stem_lower.starts_with(&format!("{stem_lower}."))
        {
            found.push(path);
        }
    }

    found.sort();
    found
}

fn language_hint_from_sidecar_name(media_path: &Path, sidecar: &Path) -> Option<String> {
    let media_stem = media_path.file_stem()?.to_str()?.to_ascii_lowercase();
    let side_stem = sidecar.file_stem()?.to_str()?.to_ascii_lowercase();
    if side_stem == media_stem {
        return None;
    }
    let prefix = format!("{media_stem}.");
    side_stem.strip_prefix(&prefix).map(|rest| rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn discovers_same_stem_sidecar() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("lumina-sub-discover-{stamp}"));
        let _ = fs::create_dir_all(&dir);
        let video = dir.join("movie.mkv");
        let sub = dir.join("movie.srt");
        let sub_lang = dir.join("movie.en.srt");
        let other = dir.join("other.srt");
        let _ = fs::write(&video, b"x");
        let _ = fs::write(&sub, b"1\n00:00:01,000 --> 00:00:02,000\nHi\n");
        let _ = fs::write(&sub_lang, b"1\n00:00:01,000 --> 00:00:02,000\nHi\n");
        let _ = fs::write(&other, b"1\n00:00:01,000 --> 00:00:02,000\nHi\n");

        let found = discover_sidecars(&video);
        let names: Vec<_> = found
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
            .collect();
        let _ = fs::remove_dir_all(&dir);

        assert!(names.iter().any(|n| n == "movie.srt"));
        assert!(names.iter().any(|n| n == "movie.en.srt"));
        assert!(!names.iter().any(|n| n == "other.srt"));
    }
}
