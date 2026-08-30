//! Extract embedded subtitle tracks with project-local ffmpeg.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::media::tools::resolve_ffmpeg;
use crate::subtitle::error::SubtitleError;

const BITMAP_CODECS: &[&str] = &["hdmv_pgs_subtitle", "pgssub", "dvd_subtitle", "dvb_subtitle", "xsub"];

pub fn is_bitmap_codec(codec: Option<&str>) -> bool {
    codec
        .map(|c| BITMAP_CODECS.iter().any(|b| c.eq_ignore_ascii_case(b)))
        .unwrap_or(false)
}

pub fn extract_text_subtitle(
    media_path: &Path,
    stream_index: u32,
    codec_name: Option<&str>,
) -> Result<(String, &'static str), SubtitleError> {
    if is_bitmap_codec(codec_name) {
        return Err(SubtitleError::unsupported(
            "bitmap subtitles are not supported in Phase 3 (no OCR)",
            codec_name,
        ));
    }

    let ffmpeg = resolve_ffmpeg().map_err(SubtitleError::from)?;
    let temp_dir = std::env::temp_dir().join("lumina-subs");
    fs::create_dir_all(&temp_dir).map_err(|error| {
        SubtitleError::internal("failed to create temp subtitle dir", Some(&error.to_string()))
    })?;

    let stem = media_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    let safe_stem: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(40)
        .collect();

    // Prefer SRT; fall back to ASS.
    for (ext, codec_arg) in [("srt", "srt"), ("ass", "ass")] {
        let out = temp_dir.join(format!("{safe_stem}_{stream_index}.{ext}"));
        if out.exists() {
            let _ = fs::remove_file(&out);
        }

        let map = format!("0:{stream_index}");
        let output = Command::new(&ffmpeg)
            .args([
                "-y",
                "-i",
                &media_path.to_string_lossy(),
                "-map",
                &map,
                "-c:s",
                codec_arg,
                &out.to_string_lossy(),
            ])
            .output()
            .map_err(|error| {
                SubtitleError::extract_failed("failed to spawn ffmpeg", Some(&error.to_string()))
            })?;

        if output.status.success() && out.is_file() {
            let content = fs::read_to_string(&out).map_err(|error| {
                SubtitleError::extract_failed(
                    "failed to read extracted subtitle file",
                    Some(&error.to_string()),
                )
            })?;
            let _ = fs::remove_file(&out);
            if content.trim().is_empty() {
                continue;
            }
            tracing::info!(
                stream_index,
                format = ext,
                bytes = content.len(),
                "subtitle track extracted"
            );
            return Ok((content, ext));
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            stream_index,
            format = ext,
            %stderr,
            "ffmpeg subtitle extract attempt failed"
        );
        let _ = fs::remove_file(&out);
    }

    Err(SubtitleError::extract_failed(
        "failed to extract text subtitle track",
        Some("ffmpeg could not convert this track to SRT/ASS"),
    ))
}

pub fn read_external_subtitle(path: &Path) -> Result<(String, PathBuf), SubtitleError> {
    if !path.is_file() {
        return Err(SubtitleError::file_not_found(&path.to_string_lossy()));
    }
    let content = fs::read_to_string(path).map_err(|error| {
        SubtitleError::extract_failed(
            "failed to read external subtitle file",
            Some(&error.to_string()),
        )
    })?;
    Ok((content, path.to_path_buf()))
}
