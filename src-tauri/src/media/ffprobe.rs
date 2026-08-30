//! Resolve and run project-local ffprobe.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::media::error::MediaError;
use crate::media::model::{MediaInfo, MediaStream, StreamKind};
use crate::media::tools::resolve_ffprobe;

#[derive(Debug, Deserialize)]
struct ProbeJson {
    format: Option<ProbeFormat>,
    streams: Option<Vec<ProbeStream>>,
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
            &path.to_string_lossy(),
        ])
        .output()
        .map_err(|error| {
            MediaError::probe_failed("failed to spawn ffprobe", Some(&error.to_string()))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(MediaError::probe_failed(
            "ffprobe exited with error",
            Some(if stderr.is_empty() {
                "non-zero exit status"
            } else {
                &stderr
            }),
        ));
    }

    let parsed: ProbeJson = serde_json::from_slice(&output.stdout).map_err(|error| {
        MediaError::probe_failed("failed to parse ffprobe JSON", Some(&error.to_string()))
    })?;

    let format = parsed
        .format
        .ok_or_else(|| MediaError::invalid_media("ffprobe returned no format section", None))?;

    let streams = parsed
        .streams
        .unwrap_or_default()
        .into_iter()
        .map(map_stream)
        .collect::<Vec<_>>();

    if streams.is_empty() {
        return Err(MediaError::invalid_media(
            "no streams found in media file",
            None,
        ));
    }

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
    })
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
    }

    #[test]
    fn resolve_finds_project_ffprobe() {
        let path = resolve_ffprobe();
        assert!(
            path.is_ok(),
            "expected native/ffmpeg/ffprobe.exe: {path:?}"
        );
    }
}
