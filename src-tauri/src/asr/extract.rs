//! Extract mono 16kHz wav for whisper using project-local ffmpeg.

use std::path::{Path, PathBuf};

use crate::asr::error::AsrError;
use crate::media::tools::resolve_ffmpeg;
use crate::process_util::command;

/// Optional half-open media window `[from_ms, to_ms)`.
pub fn extract_wav_16k_mono(
    media_path: &Path,
    out_wav: &Path,
    range: Option<(u64, u64)>,
) -> Result<(), AsrError> {
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
        ?range,
        "extracting audio for ASR"
    );

    let mut cmd = command(&ffmpeg);
    cmd.arg("-y");
    if let Some((from_ms, to_ms)) = range {
        if to_ms <= from_ms {
            return Err(AsrError::invalid("转写时间范围无效"));
        }
        let start_sec = from_ms as f64 / 1000.0;
        let duration_sec = (to_ms - from_ms) as f64 / 1000.0;
        cmd.args(["-ss", &format!("{start_sec:.3}")]);
        cmd.arg("-i").arg(media_path);
        cmd.args(["-t", &format!("{duration_sec:.3}")]);
    } else {
        cmd.arg("-i").arg(media_path);
    }
    cmd.args([
        "-vn",
        "-ac",
        "1",
        "-ar",
        "16000",
        "-c:a",
        "pcm_s16le",
    ]);
    cmd.arg(out_wav);

    let output = cmd.output().map_err(|error| {
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
