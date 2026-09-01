//! Extract compressed JPEG frames via project-local ffmpeg.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::media::tools::resolve_ffmpeg;
use crate::media::MediaError;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_FRAME_WIDTH: u32 = 640;

pub fn capture_frames(
    media_path: &Path,
    sample_times_sec: &[f64],
    output_dir: &Path,
) -> Result<Vec<PathBuf>, MediaError> {
    if sample_times_sec.is_empty() {
        return Ok(Vec::new());
    }
    let ffmpeg = resolve_ffmpeg()?;
    std::fs::create_dir_all(output_dir).map_err(|error| {
        MediaError::internal(Some(&format!("create capture dir: {error}")))
    })?;

    let mut outputs = Vec::new();
    for (index, time_sec) in sample_times_sec.iter().enumerate() {
        let output = output_dir.join(format!("frame-{index:02}.jpg"));
        let started = Instant::now();
        let status = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                &format!("{time_sec:.3}"),
                "-i",
            ])
            .arg(media_path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                &format!("scale={MAX_FRAME_WIDTH}:-1"),
                "-q:v",
                "5",
                "-y",
            ])
            .arg(&output)
            .status()
            .map_err(|error| MediaError::internal(Some(&format!("spawn ffmpeg: {error}"))))?;
        if started.elapsed() > CAPTURE_TIMEOUT {
            return Err(MediaError::internal(Some("capture timed out")));
        }
        if !status.success() || !output.is_file() {
            return Err(MediaError::internal(Some("capture failed")));
        }
        outputs.push(output);
    }
    Ok(outputs)
}

pub fn sample_times_for_window(
    center_ms: u64,
    duration_ms: Option<u64>,
    before_sec: u32,
    after_sec: u32,
) -> Vec<f64> {
    let center_sec = center_ms as f64 / 1000.0;
    let max_sec = duration_ms
        .map(|ms| ms as f64 / 1000.0)
        .unwrap_or(f64::MAX);
    let mut times = Vec::new();
    if before_sec == 0 && after_sec == 0 {
        times.push(center_sec.clamp(0.0, max_sec));
        return times;
    }
    if before_sec > 0 {
        times.push((center_sec - before_sec as f64).clamp(0.0, max_sec));
    }
    times.push(center_sec.clamp(0.0, max_sec));
    if after_sec > 0 {
        times.push((center_sec + after_sec as f64).clamp(0.0, max_sec));
    }
    times.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|left, right| (*left - *right).abs() < 0.05);
    times.truncate(3);
    times
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_times_default_is_anchor_only() {
        assert_eq!(sample_times_for_window(30_000, Some(120_000), 0, 0), vec![30.0]);
    }

    #[test]
    fn sample_times_asymmetric_window() {
        let times = sample_times_for_window(30_000, Some(120_000), 3, 2);
        assert_eq!(times, vec![27.0, 30.0, 32.0]);
    }
}
