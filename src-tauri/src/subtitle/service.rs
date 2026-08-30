//! SubtitleService — list tracks and build transcripts.

use std::path::Path;

use crate::media::{MediaInspector, StreamKind};
use crate::subtitle::error::SubtitleError;
use crate::subtitle::extract::{self, is_bitmap_codec};
use crate::subtitle::model::{SubtitleTrackInfo, Transcript};
use crate::subtitle::parse::parse_subtitle_text;

pub struct SubtitleService;

impl SubtitleService {
    pub fn list_tracks(path: impl AsRef<Path>) -> Result<Vec<SubtitleTrackInfo>, SubtitleError> {
        let info = MediaInspector::inspect(path).map_err(SubtitleError::from)?;
        let tracks: Vec<_> = info
            .streams
            .iter()
            .filter(|s| s.kind == StreamKind::Subtitle)
            .map(|s| SubtitleTrackInfo {
                stream_index: s.index,
                codec_name: s.codec_name.clone(),
                language: s.language.clone(),
                title: None,
            })
            .collect();
        if tracks.is_empty() {
            return Err(SubtitleError::no_track());
        }
        Ok(tracks)
    }

    pub fn load_from_media(
        path: impl AsRef<Path>,
        stream_index: u32,
    ) -> Result<Transcript, SubtitleError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(SubtitleError::file_not_found(&path.to_string_lossy()));
        }

        let tracks = Self::list_tracks(path)?;
        let track = tracks
            .iter()
            .find(|t| t.stream_index == stream_index)
            .ok_or_else(|| {
                SubtitleError::extract_failed(
                    "requested subtitle stream index not found",
                    Some(&stream_index.to_string()),
                )
            })?;

        if is_bitmap_codec(track.codec_name.as_deref()) {
            return Err(SubtitleError::unsupported(
                "bitmap subtitles are not supported in Phase 3 (no OCR)",
                track.codec_name.as_deref(),
            ));
        }

        let (content, _format) =
            extract::extract_text_subtitle(path, stream_index, track.codec_name.as_deref())?;
        let cues = parse_subtitle_text(&content)?;

        Ok(Transcript {
            source_path: path.to_string_lossy().to_string(),
            stream_index: Some(stream_index),
            language: track.language.clone(),
            codec_name: track.codec_name.clone(),
            cues,
        })
    }

    pub fn load_external(path: impl AsRef<Path>) -> Result<Transcript, SubtitleError> {
        let path = path.as_ref();
        let (content, path_buf) = extract::read_external_subtitle(path)?;
        let cues = parse_subtitle_text(&content)?;
        Ok(Transcript {
            source_path: path_buf.to_string_lossy().to_string(),
            stream_index: None,
            language: None,
            codec_name: path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_string),
            cues,
        })
    }
}
