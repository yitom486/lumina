//! Extract mono 16kHz wav for whisper using project-local ffmpeg.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::asr::error::AsrError;
use crate::media::tools::resolve_ffmpeg;

pub fn extract_wav_16k_mono(media_path: &Path, out_wav: &Path) -> Result<(), AsrError> {
    let ffmpeg = resolve_ffmpeg().map_err(|error| {
        tracing::warn!(%error, "ffmpeg missing for ASR extract");
        AsrError::extract_failed(Some(&format!("ffmpeg not found: {error}")))
    })?;

    if let Some(parent) = out_wav.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            tracing::warn!(%error, "ASR temp dir create failed");
            AsrError::extract_failed(Some(&format!("create temp audio dir: {error}")))
        })?;
    }

    if out_wav.exists() {
        let _ = std::fs::remove_file(out_wav);
    }

    tracing::info!(
        ffmpeg = %ffmpeg.display(),
        media = %media_path.display(),
        out = %out_wav.display(),
        "extracting audio for ASR"
    );

    let output = Command::new(&ffmpeg)
        .args([
            "-y",
            "-i",
            &media_path.to_string_lossy(),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            &out_wav.to_string_lossy(),
        ])
        .output()
        .map_err(|error| {
            tracing::warn!(%error, "ffmpeg spawn failed for ASR");
            AsrError::extract_failed(Some(&format!("ffmpeg spawn: {error}")))
        })?;

    if !output.status.success() || !out_wav.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(%stderr, "ffmpeg ASR extract failed");
        return Err(AsrError::extract_failed(Some(stderr.trim())));
    }

    Ok(())
}

pub fn temp_job_dir(media_path: &Path) -> PathBuf {
    let stem = media_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    let safe: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(40)
        .collect();
    std::env::temp_dir().join("lumina-asr").join(safe)
}
