//! Headless playback baseline (H-P1-5, half of Phase 8.0).
//!
//! Manual only: `cargo test --lib baseline_local_playback -- --ignored --nocapture`
//!
//! Env:
//! - `LUMINA_BASELINE_MEDIA`: media file to measure. When unset, a 1080p60/10s
//!   fixture is generated with the vendored ffmpeg (SKIP when ffmpeg is missing).
//! - `LUMINA_BASELINE_STEADY_MS`: steady pacing window, default 5000.
//! - `LUMINA_BASELINE_OUT`: optional path to also write the JSON report.
//!
//! What it measures (all headless `vo=null`, real decode):
//! demux-ready latency, first-frame latency (`time-pos > 0`), steady pacing
//! rate, three seek latencies, decoder/vo drop counts, last avsync, and the
//! `hwdec-current` context (headless often differs from the wid product path —
//! only compare runs taken under identical conditions).
//! OS-level CPU/GPU sampling stays manual (see ARCHITECTURE baseline section).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::player::LibMpvPlayer;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BaselineSeek {
    target_ms: u64,
    latency_ms: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BaselineReport {
    media: String,
    width: Option<u32>,
    height: Option<u32>,
    codec: Option<String>,
    duration_ms: Option<u64>,
    hwdec: Option<String>,
    demux_ready_ms: Option<u64>,
    first_frame_ms: u64,
    steady_window_ms: u64,
    steady_advanced_ms: i64,
    seeks: Vec<BaselineSeek>,
    decoder_drops: i64,
    vo_drops: i64,
    avsync_last: Option<f64>,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn generate_fixture() -> Option<PathBuf> {
    let ffmpeg = crate::media::tools::resolve_ffmpeg().ok()?;
    let dir = std::env::temp_dir().join(format!("lumina-baseline-{}", std::process::id()));
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let out = dir.join("baseline-1080p60.mp4");
    let status = crate::process_util::command(&ffmpeg)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=10:size=1920x1080:rate=60",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=10",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ])
        .arg(&out)
        .output()
        .ok()?;
    if status.status.success() && out.is_file() {
        Some(out)
    } else {
        None
    }
}

fn poll_until(timeout: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    ready()
}

#[test]
#[ignore = "manual baseline: needs seconds of headless decode"]
fn baseline_local_playback() {
    let media = match env_path("LUMINA_BASELINE_MEDIA") {
        Some(path) => path,
        None => match generate_fixture() {
            Some(path) => path,
            None => {
                eprintln!("SKIP baseline: no LUMINA_BASELINE_MEDIA and ffmpeg unavailable");
                return;
            }
        },
    };
    assert!(
        media.is_file(),
        "baseline media missing: {}",
        media.display()
    );

    let steady_window_ms: u64 = std::env::var("LUMINA_BASELINE_STEADY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000);

    let probe = crate::media::service::MediaInspector::inspect(&media).ok();
    let video = probe
        .as_ref()
        .and_then(|info| info.primary_video())
        .cloned();

    let player = LibMpvPlayer::initialize().expect("headless libmpv should initialize");
    if let Err(error) = player.enable_hwdec() {
        eprintln!("baseline: hwdec=auto rejected ({error}); continuing without it");
    }

    let media_arg = media.to_string_lossy().into_owned();
    let t0 = Instant::now();
    player
        .open(&media_arg)
        .expect("loadfile should be accepted");

    // Demux-ready + first frame.
    let mut demux_ready_ms = None;
    poll_until(Duration::from_secs(30), || {
        player.drain_diagnostic_events();
        if demux_ready_ms.is_none() {
            if let Ok(d) = player.duration_ms() {
                if d > 0 {
                    demux_ready_ms = Some(t0.elapsed().as_millis() as u64);
                }
            }
        }
        player.position_ms().is_ok_and(|p| p > 0)
    });
    let first_frame_ms = t0.elapsed().as_millis() as u64;
    assert!(
        player.position_ms().is_ok_and(|p| p > 0),
        "no frame within 30s for {}",
        media.display()
    );

    // Steady pacing window.
    let p0 = player.position_ms().unwrap_or(0);
    let steady_start = Instant::now();
    while steady_start.elapsed() < Duration::from_millis(steady_window_ms) {
        player.drain_diagnostic_events();
        std::thread::sleep(Duration::from_millis(100));
    }
    let steady_advanced_ms = player.position_ms().unwrap_or(p0) as i64 - p0 as i64;

    // Seek latencies: start / middle / near end.
    let duration = player.duration_ms().ok().filter(|d| *d > 0).or_else(|| {
        probe
            .as_ref()
            .and_then(|info| info.duration_ms)
            .filter(|d| *d > 0)
    });
    let mut seeks = Vec::new();
    if let Some(duration) = duration {
        for target in [0, duration / 2, duration.saturating_sub(5000)] {
            player.seek_ms(target).expect("seek should be accepted");
            let t = Instant::now();
            let settled = poll_until(Duration::from_secs(10), || {
                player.drain_diagnostic_events();
                player
                    .position_ms()
                    .is_ok_and(|p| p.abs_diff(target) <= 500)
            });
            seeks.push(BaselineSeek {
                target_ms: target,
                latency_ms: settled.then(|| t.elapsed().as_millis() as u64),
            });
        }
    }

    let (decoder_drops, vo_drops) = player.frame_drop_counts();
    let report = BaselineReport {
        media: media.display().to_string(),
        width: video.as_ref().and_then(|v| v.width),
        height: video.as_ref().and_then(|v| v.height),
        codec: video.as_ref().and_then(|v| v.codec_name.clone()),
        duration_ms: duration,
        hwdec: player.hwdec_current(),
        demux_ready_ms,
        first_frame_ms,
        steady_window_ms,
        steady_advanced_ms,
        seeks,
        decoder_drops,
        vo_drops,
        avsync_last: player.avsync_last(),
    };
    let json = serde_json::to_string_pretty(&report).expect("baseline report should serialize");
    println!("{json}");
    if let Some(out) = env_path("LUMINA_BASELINE_OUT") {
        if let Err(error) = std::fs::write(&out, &json) {
            eprintln!("baseline: failed to write {}: {error}", out.display());
        }
    }

    player.stop().ok();
}
