//! Resolve and run project-local ffprobe.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::media::error::MediaError;
use crate::media::model::{MediaChapter, MediaInfo, MediaStream, StreamKind};
use crate::media::tools::resolve_ffprobe_with;
use crate::process_util::command;

#[derive(Debug, Deserialize)]
struct ProbeJson {
    format: Option<ProbeFormat>,
    streams: Option<Vec<ProbeStream>>,
    chapters: Option<Vec<ProbeChapter>>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    filename: Option<String>,
    format_name: Option<String>,
    format_long_name: Option<String>,
    duration: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    index: Option<u32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    codec_long_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    bit_rate: Option<String>,
    tags: Option<ProbeTags>,
}

#[derive(Debug, Deserialize)]
struct ProbeTags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeChapter {
    /// ffprobe may emit huge/negative ids; accept loosely and fall back when mapping.
    #[serde(default)]
    id: Option<serde_json::Value>,
    start_time: Option<String>,
    end_time: Option<String>,
    tags: Option<ProbeTags>,
}

/// 带 resource_dir 的版本，打包后调用。
pub fn probe_file_with(
    path: &Path,
    resource_dir: Option<&PathBuf>,
) -> Result<MediaInfo, MediaError> {
    if !path.is_file() {
        return Err(MediaError::file_not_found(&path.to_string_lossy()));
    }

    let ffprobe = resolve_ffprobe_with(resource_dir)?;
    run_probe(path, &ffprobe)
}

pub fn probe_file(path: &Path) -> Result<MediaInfo, MediaError> {
    probe_file_with(path, None)
}

fn run_probe(path: &Path, ffprobe: &PathBuf) -> Result<MediaInfo, MediaError> {
    tracing::info!(
        ffprobe = %ffprobe.display(),
        path = %path.display(),
        "running ffprobe"
    );

    let output = command(ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
            &path.to_string_lossy(),
        ])
        .output()
        .map_err(|error| {
            tracing::warn!(%error, "ffprobe spawn failed");
            MediaError::probe_failed(Some(&error.to_string()))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let details = if stderr.is_empty() {
            "ffprobe non-zero exit status".to_string()
        } else {
            format!("ffprobe stderr: {stderr}")
        };
        tracing::warn!(%details, "ffprobe exited with error");
        return Err(MediaError::probe_failed(Some(&details)));
    }

    let parsed: ProbeJson = serde_json::from_slice(&output.stdout).map_err(|error| {
        tracing::warn!(
            %error,
            stdout_len = output.stdout.len(),
            "ffprobe json parse failed"
        );
        MediaError::probe_failed(Some(&format!("ffprobe json: {error}")))
    })?;

    let format = parsed.format.ok_or_else(|| {
        tracing::warn!("ffprobe json missing format section");
        MediaError::invalid_media(Some("ffprobe returned no format section"))
    })?;

    let streams = parsed
        .streams
        .unwrap_or_default()
        .into_iter()
        .map(map_stream)
        .collect::<Vec<_>>();

    if streams.is_empty() {
        tracing::warn!("ffprobe returned zero streams");
        return Err(MediaError::invalid_media(Some(
            "no streams found in media file",
        )));
    }

    let chapters = parsed
        .chapters
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(i, chapter)| map_chapter(i as u32, chapter))
        .collect::<Vec<_>>();

    Ok(MediaInfo {
        path: format
            .filename
            .unwrap_or_else(|| path.to_string_lossy().to_string()),
        format_name: format.format_name,
        format_long_name: format.format_long_name,
        duration_ms: parse_secs_to_ms(format.duration.as_deref()),
        size_bytes: parse_u64(format.size.as_deref()),
        bit_rate: parse_u64(format.bit_rate.as_deref()),
        streams,
        chapters,
    })
}

fn map_chapter(fallback_id: u32, chapter: ProbeChapter) -> Option<MediaChapter> {
    let start_ms = parse_secs_to_ms(chapter.start_time.as_deref())?;
    let end_ms = parse_secs_to_ms(chapter.end_time.as_deref());
    let title = chapter.tags.and_then(|t| t.title);
    Some(MediaChapter {
        id: parse_flexible_u32(chapter.id).unwrap_or(fallback_id),
        start_ms,
        end_ms,
        title,
    })
}

fn parse_flexible_u32(value: Option<serde_json::Value>) -> Option<u32> {
    match value? {
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                if u <= u32::MAX as u64 {
                    return Some(u as u32);
                }
            }
            if let Some(i) = n.as_i64() {
                if (0..=u32::MAX as i64).contains(&i) {
                    return Some(i as u32);
                }
            }
            None
        }
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn map_stream(stream: ProbeStream) -> MediaStream {
    let kind = match stream.codec_type.as_deref() {
        Some("video") => StreamKind::Video,
        Some("audio") => StreamKind::Audio,
        Some("subtitle") => StreamKind::Subtitle,
        Some("data") => StreamKind::Data,
        Some("attachment") => StreamKind::Attachment,
        _ => StreamKind::Unknown,
    };

    let frame_rate = stream
        .avg_frame_rate
        .as_deref()
        .or(stream.r_frame_rate.as_deref())
        .and_then(parse_frame_rate);

    MediaStream {
        index: stream.index.unwrap_or(0),
        kind,
        codec_name: stream.codec_name,
        codec_long_name: stream.codec_long_name,
        width: stream.width,
        height: stream.height,
        frame_rate,
        sample_rate: stream.sample_rate.as_deref().and_then(|s| s.parse().ok()),
        channels: stream.channels,
        bit_rate: parse_u64(stream.bit_rate.as_deref()),
        language: stream.tags.and_then(|t| t.language),
    }
}

fn parse_secs_to_ms(value: Option<&str>) -> Option<u64> {
    let secs: f64 = value?.parse().ok()?;
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    Some((secs * 1000.0) as u64)
}

fn parse_u64(value: Option<&str>) -> Option<u64> {
    value?.parse().ok()
}

fn parse_frame_rate(value: &str) -> Option<f64> {
    if value == "0/0" || value.is_empty() {
        return None;
    }
    if let Some((num, den)) = value.split_once('/') {
        let n: f64 = num.parse().ok()?;
        let d: f64 = den.parse().ok()?;
        if d == 0.0 {
            return None;
        }
        let rate = n / d;
        if rate.is_finite() && rate > 0.0 {
            return Some(rate);
        }
        return None;
    }
    value.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::tools::resolve_ffprobe;

    #[test]
    fn frame_rate_fraction() {
        assert!((parse_frame_rate("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert!(parse_frame_rate("0/0").is_none());
        assert!(parse_frame_rate("").is_none());
        assert_eq!(parse_frame_rate("24").unwrap(), 24.0);
    }

    #[test]
    fn parse_duration_helpers() {
        assert_eq!(parse_secs_to_ms(Some("1.5")), Some(1500));
        assert_eq!(parse_secs_to_ms(Some("-1")), None);
        assert_eq!(parse_u64(Some("42")), Some(42));
        assert_eq!(parse_u64(Some("x")), None);
    }

    #[test]
    fn resolve_finds_project_ffprobe() {
        match resolve_ffprobe() {
            Ok(_) => {}
            Err(error) if cfg!(windows) => {
                panic!("expected native/ffmpeg/ffprobe.exe: {error:?}");
            }
            Err(_) => {
                // Unix CI may not vendor ffprobe; resolution still succeeds when present.
            }
        }
    }

    fn encoder_available(ffmpeg: &std::path::Path, encoder: &str) -> bool {
        let spec = format!("encoder={encoder}");
        crate::process_util::command(ffmpeg)
            .args(["-hide_banner", "-h", spec.as_str()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn codec_matrix_probes_h264_hevc_av1() {
        // Probe-level matrix only: mpv playback stays on the same loadfile path
        // for every codec. Unix machines without vendored ffmpeg SKIP (same
        // convention as `resolve_finds_project_ffprobe`).
        let ffmpeg = match crate::media::tools::resolve_ffmpeg() {
            Ok(path) => path,
            Err(_) => {
                eprintln!("SKIP codec matrix: ffmpeg not vendored on this machine");
                return;
            }
        };
        // ffprobe travels with ffmpeg; its absence beside a present ffmpeg is real.
        assert!(
            crate::media::tools::resolve_ffprobe().is_ok(),
            "ffmpeg resolved but ffprobe is missing next to it"
        );

        let dir = std::env::temp_dir().join(format!("lumina-codec-matrix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("matrix temp dir");

        // (label, encoder, extra args, container, expected codec substring)
        let cases: &[(&str, &str, &[&str], &str, &str)] = &[
            ("h264", "libx264", &["-preset", "veryfast"], "mp4", "h264"),
            ("hevc", "libx265", &["-preset", "ultrafast"], "mp4", "hevc"),
            (
                "av1",
                "libaom-av1",
                &["-cpu-used", "8", "-crf", "30"],
                "mkv",
                "av1",
            ),
        ];
        for (label, encoder, extra, ext, expected) in cases {
            if !encoder_available(&ffmpeg, encoder) {
                eprintln!("SKIP codec matrix {label}: encoder {encoder} not in this ffmpeg build");
                continue;
            }
            let out = dir.join(format!("matrix-{label}.{ext}"));
            let output = crate::process_util::command(&ffmpeg)
                .args([
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc=duration=1:size=64x64:rate=10",
                ])
                .args(["-c:v", encoder])
                .args(*extra)
                .args(["-pix_fmt", "yuv420p", "-an"])
                .arg(&out)
                .output()
                .expect("spawn ffmpeg");
            assert!(
                output.status.success(),
                "{label} fixture failed to encode: {}",
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(2000)
                    .collect::<String>()
            );
            let info = crate::media::service::MediaInspector::inspect(&out).expect("probe fixture");
            let video = info
                .streams
                .iter()
                .find(|s| s.kind == crate::media::model::StreamKind::Video)
                .expect("video stream");
            let codec = video.codec_name.as_deref().unwrap_or("");
            assert!(
                codec.contains(expected),
                "{label}: expected codec containing {expected}, got {codec:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_chinese_not_found() {
        let err = probe_file(Path::new("Z:\\lumina-missing-media-xyz.mp4")).expect_err("missing");
        assert_eq!(err.code, crate::media::error::MediaErrorCode::FileNotFound);
        assert!(err
            .message
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
    }

    #[test]
    fn flexible_u32_ignores_huge_negative() {
        assert_eq!(
            parse_flexible_u32(Some(serde_json::json!(-2481607921214808748_i64))),
            None
        );
        assert_eq!(parse_flexible_u32(Some(serde_json::json!(3))), Some(3));
        assert_eq!(parse_flexible_u32(Some(serde_json::json!("7"))), Some(7));
    }
}
