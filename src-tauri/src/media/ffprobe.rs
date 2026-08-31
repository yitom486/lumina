//! Resolve and run project-local ffprobe.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::media::error::MediaError;
use crate::media::model::{MediaChapter, MediaInfo, MediaStream, StreamKind};
use crate::media::tools::resolve_ffprobe;

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

pub fn probe_file(path: &Path) -> Result<MediaInfo, MediaError> {
    if !path.is_file() {
        return Err(MediaError::file_not_found(&path.to_string_lossy()));
    }

    let ffprobe = resolve_ffprobe()?;
    tracing::info!(
        ffprobe = %ffprobe.display(),
        path = %path.display(),
        "running ffprobe"
    );

    let output = Command::new(&ffprobe)
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
        let path = resolve_ffprobe();
        assert!(path.is_ok(), "expected native/ffmpeg/ffprobe.exe: {path:?}");
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
