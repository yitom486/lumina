//! Extract compressed JPEG frames via project-local ffmpeg.
//!
//! Product rules (`.plan/L1-mcp-adaptive-tools.md`):
//! - Anchor = playback position when the user starts typing the prompt (10s idle resets)
//! - Default: one frame at anchor; optional windows sample ~1 frame/sec (max 15)
//! - Scale to 640px width, JPEG `-q:v 5` for moderate size
//! - Write under `.lumina/tmp/capture-*`; deleted after MCP returns (next prompt also clears tmp)

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::media::tools::resolve_ffmpeg;
use crate::media::MediaError;
use crate::process_util::command;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_FRAME_WIDTH: u32 = 640;
/// At most one sample per second in the capture window (see `sample_times_for_window`).
pub const MAX_CAPTURE_FRAMES: usize = 15;
/// Per-direction span cap at 1 fps → up to 7 + anchor + 7 = 15 frames.
pub const MAX_CAPTURE_SPAN_SEC: u32 = 7;

/// P7-S1 measured budget model. Spike (2026-09-06, 640px JPEG q:v 5):
/// 15 frames = 143 KiB total (~9.5 KiB/frame) in 1.8 s wall.
/// Caps are generous headroom over measured, not tuned optima.
/// Selection algorithms come after Spike review — this only judges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameBudget {
    pub max_frames: usize,
    pub max_width: u32,
    pub max_total_bytes: u64,
    pub max_window_sec: u32,
}

pub const DEFAULT_FRAME_BUDGET: FrameBudget = FrameBudget {
    max_frames: MAX_CAPTURE_FRAMES,
    max_width: MAX_FRAME_WIDTH,
    max_total_bytes: 4 * 1024 * 1024,
    max_window_sec: 2 * MAX_CAPTURE_SPAN_SEC,
};

/// Pure budget check over a capture result: count, bulk, width, window span.
pub fn within_budget(
    frame_count: usize,
    total_bytes: u64,
    width: u32,
    window_sec: u32,
    budget: FrameBudget,
) -> bool {
    frame_count <= budget.max_frames
        && total_bytes <= budget.max_total_bytes
        && width <= budget.max_width
        && window_sec <= budget.max_window_sec
}

pub fn capture_frames(
    media_path: &Path,
    sample_times_sec: &[f64],
    output_dir: &Path,
) -> Result<Vec<PathBuf>, MediaError> {
    if sample_times_sec.is_empty() {
        return Ok(Vec::new());
    }
    let ffmpeg = resolve_ffmpeg()?;
    std::fs::create_dir_all(output_dir)
        .map_err(|error| MediaError::internal(Some(&format!("create capture dir: {error}"))))?;

    let mut outputs = Vec::new();
    for (index, time_sec) in sample_times_sec.iter().enumerate() {
        let output = output_dir.join(format!("frame-{index:02}.jpg"));
        let started = Instant::now();
        let status = command(&ffmpeg)
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
    let max_sec = duration_ms.map(|ms| ms as f64 / 1000.0).unwrap_or(f64::MAX);
    if before_sec == 0 && after_sec == 0 {
        return vec![center_sec.clamp(0.0, max_sec)];
    }

    let window_start = (center_sec - f64::from(before_sec)).clamp(0.0, max_sec);
    let window_end = (center_sec + f64::from(after_sec)).clamp(0.0, max_sec);
    let mut times = Vec::new();
    let mut sec = window_start.floor();
    while sec <= window_end + f64::EPSILON {
        times.push(sec.clamp(0.0, max_sec));
        sec += 1.0;
    }
    if times.is_empty() {
        times.push(center_sec.clamp(0.0, max_sec));
    }
    times.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|left, right| (*left - *right).abs() < 0.05);
    if times.len() > MAX_CAPTURE_FRAMES {
        times = subsample_times(times, MAX_CAPTURE_FRAMES);
    }
    times
}

fn subsample_times(times: Vec<f64>, max_len: usize) -> Vec<f64> {
    if times.len() <= max_len {
        return times;
    }
    if max_len == 0 {
        return Vec::new();
    }
    if max_len == 1 {
        return vec![times[times.len() / 2]];
    }
    let last_index = times.len() - 1;
    (0..max_len)
        .map(|index| {
            let pick = index * last_index / (max_len - 1);
            times[pick]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_times_default_is_anchor_only() {
        assert_eq!(
            sample_times_for_window(30_000, Some(120_000), 0, 0),
            vec![30.0]
        );
    }

    #[test]
    fn sample_times_asymmetric_window_samples_one_per_second() {
        let times = sample_times_for_window(30_000, Some(120_000), 3, 2);
        assert_eq!(times, vec![27.0, 28.0, 29.0, 30.0, 31.0, 32.0]);
    }

    #[test]
    fn sample_times_symmetric_radius_samples_one_per_second() {
        let times = sample_times_for_window(30_000, Some(120_000), 3, 3);
        assert_eq!(times, vec![27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0]);
    }

    #[test]
    fn sample_times_respects_max_frame_cap() {
        let times = sample_times_for_window(600_000, Some(1_800_000), 10, 10);
        assert_eq!(times.len(), MAX_CAPTURE_FRAMES);
    }

    #[test]
    fn budget_judges_count_bulk_width_and_window() {
        let ok = DEFAULT_FRAME_BUDGET;
        assert!(within_budget(15, 143_000, 640, 14, ok));
        assert!(within_budget(1, 10_000, 640, 0, ok));
        assert!(!within_budget(16, 143_000, 640, 14, ok));
        assert!(!within_budget(15, 9 * 1024 * 1024, 640, 14, ok));
        assert!(!within_budget(15, 143_000, 1280, 14, ok));
        assert!(!within_budget(15, 143_000, 640, 15, ok));
    }

    /// P7-S1 spike measurement: real numbers for the budget model.
    /// Prints per-run figures with `--nocapture`; asserts structure only.
    #[test]
    fn spike_measures_fifteen_frame_capture() {
        let ffmpeg = match resolve_ffmpeg() {
            Ok(path) => path,
            Err(_) => {
                eprintln!("SKIP frame spike: ffmpeg not vendored on this machine");
                return;
            }
        };
        let dir = std::env::temp_dir().join(format!("lumina-frame-spike-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("spike temp dir");
        let media = dir.join("spike-20s.mp4");
        let status = command(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=20:size=640x360:rate=30",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-an",
            ])
            .arg(&media)
            .output()
            .expect("spawn ffmpeg");
        assert!(status.status.success(), "spike fixture failed to encode");

        let times = sample_times_for_window(10_000, Some(20_000), 7, 7);
        assert_eq!(times.len(), MAX_CAPTURE_FRAMES);
        let out_dir = dir.join("frames");
        let started = Instant::now();
        let frames = capture_frames(&media, &times, &out_dir).expect("capture 15 frames");
        let wall_ms = started.elapsed().as_millis();
        assert_eq!(frames.len(), MAX_CAPTURE_FRAMES);
        let mut total_bytes = 0u64;
        for frame in &frames {
            let size = std::fs::metadata(frame).expect("frame file").len();
            assert!(size > 0, "empty frame: {}", frame.display());
            total_bytes += size;
        }
        eprintln!(
            "SPIKE frames=15 width=640 wall_ms={wall_ms} total_bytes={total_bytes} per_frame_bytes={}",
            total_bytes / frames.len() as u64
        );
        assert!(
            total_bytes < 30 * 1024 * 1024,
            "unexpected 15-frame bulk: {total_bytes} bytes"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
