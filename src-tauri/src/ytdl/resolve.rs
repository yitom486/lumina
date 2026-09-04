//! Run yt-dlp `-J` and map JSON into domain types.

use std::path::Path;

use serde::Deserialize;

use crate::media::model::MediaChapter;
use crate::player::source::MediaSource;
use crate::process_util::command;
use crate::ytdl::error::YtdlError;
use crate::ytdl::model::{YtdlFormat, YtdlResolveResult, YtdlSubtitleTrack};
use crate::ytdl::paths::require_cli;

#[derive(Debug, Deserialize)]
struct YtdlJson {
    id: Option<String>,
    title: Option<String>,
    duration: Option<f64>,
    webpage_url: Option<String>,
    original_url: Option<String>,
    extractor: Option<String>,
    extractor_key: Option<String>,
    chapters: Option<Vec<YtdlJsonChapter>>,
    formats: Option<Vec<YtdlJsonFormat>>,
    subtitles: Option<serde_json::Map<String, serde_json::Value>>,
    automatic_captions: Option<serde_json::Map<String, serde_json::Value>>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YtdlJsonChapter {
    start_time: Option<f64>,
    end_time: Option<f64>,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YtdlJsonFormat {
    format_id: Option<String>,
    ext: Option<String>,
    height: Option<u32>,
    width: Option<u32>,
    fps: Option<f64>,
    vcodec: Option<String>,
    acodec: Option<String>,
    tbr: Option<f64>,
    format_note: Option<String>,
    url: Option<String>,
}

/// Resolve metadata + formats for a remote page URL (does not open the player).
pub fn resolve_url(page_url: &str) -> Result<YtdlResolveResult, YtdlError> {
    let source = MediaSource::parse(page_url).map_err(|e| YtdlError::invalid(e.message))?;
    if source.kind() != crate::player::source::MediaSourceKind::Remote {
        return Err(YtdlError::invalid("请提供 http(s) 在线视频链接"));
    }
    source
        .validate()
        .map_err(|e| YtdlError::invalid(e.message))?;

    let cli = require_cli()?;
    let cookies = crate::ytdl::cookies::load();
    let raw = run_dump_json(&cli, source.playback_target(), &cookies)?;
    let parsed: YtdlJson = serde_json::from_str(&raw).map_err(|error| {
        tracing::warn!(%error, "yt-dlp json parse failed");
        YtdlError::resolve_failed(Some(&format!("json parse: {error}")))
    })?;

    Ok(map_result(parsed, &source.media_id()))
}

fn run_dump_json(
    cli: &Path,
    url: &str,
    cookies: &crate::ytdl::cookies::CookieSettings,
) -> Result<String, YtdlError> {
    let mut cmd = command(cli);
    cmd.args(["-J", "--no-playlist", "--no-warnings", "--skip-download"]);
    crate::ytdl::cookies::apply_to_command(&mut cmd, cookies)?;
    cmd.arg(url);

    let output = cmd.output().map_err(|error| {
        tracing::warn!(%error, "yt-dlp spawn failed");
        YtdlError::resolve_failed(Some(&error.to_string()))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // Never log cookie file contents — stderr from yt-dlp is OK for tracing.
        tracing::warn!(%stderr, "yt-dlp non-zero exit");
        return Err(crate::ytdl::cookies::classify_resolve_stderr(&if stderr
            .is_empty()
        {
            "yt-dlp non-zero exit".into()
        } else {
            stderr
        }));
    }

    String::from_utf8(output.stdout)
        .map_err(|error| YtdlError::resolve_failed(Some(&format!("stdout utf8: {error}"))))
}

fn map_result(json: YtdlJson, fallback_media_id: &str) -> YtdlResolveResult {
    let webpage = json.webpage_url.clone().or(json.original_url.clone());
    let media_id = webpage
        .as_deref()
        .and_then(|u| MediaSource::parse(u).ok().map(|s| s.media_id()))
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| {
            json.id
                .as_ref()
                .map(|id| {
                    let key = json
                        .extractor_key
                        .as_deref()
                        .or(json.extractor.as_deref())
                        .unwrap_or("remote");
                    format!("{key}:{id}")
                })
                .unwrap_or_else(|| fallback_media_id.to_string())
        });

    let chapters = map_chapters(json.chapters.unwrap_or_default());
    let formats = map_formats(json.formats.unwrap_or_default());
    let subtitles = map_subtitle_langs(json.subtitles.as_ref())
        .into_iter()
        .chain(map_subtitle_langs(json.automatic_captions.as_ref()))
        .collect();

    let (recommended_format_id, recommended_url) = pick_recommended(&formats, json.url.as_deref());

    YtdlResolveResult {
        media_id,
        title: json.title,
        duration_ms: json.duration.map(|s| (s * 1000.0).round() as u64),
        webpage_url: webpage,
        extractor: json.extractor.or(json.extractor_key),
        chapters,
        formats,
        subtitles,
        recommended_url,
        recommended_format_id,
    }
}

fn map_chapters(raw: Vec<YtdlJsonChapter>) -> Vec<MediaChapter> {
    raw.into_iter()
        .enumerate()
        .filter_map(|(i, ch)| {
            let start = ch.start_time?;
            Some(MediaChapter {
                id: i as u32,
                start_ms: (start * 1000.0).round() as u64,
                end_ms: ch.end_time.map(|t| (t * 1000.0).round() as u64),
                title: ch.title,
            })
        })
        .collect()
}

fn map_formats(raw: Vec<YtdlJsonFormat>) -> Vec<YtdlFormat> {
    raw.into_iter()
        .filter_map(|f| {
            let format_id = f.format_id?;
            Some(YtdlFormat {
                format_id,
                ext: f.ext,
                height: f.height,
                width: f.width,
                fps: f.fps,
                vcodec: f.vcodec,
                acodec: f.acodec,
                tbr: f.tbr,
                format_note: f.format_note,
                url: f.url,
            })
        })
        .collect()
}

fn map_subtitle_langs(
    map: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Vec<YtdlSubtitleTrack> {
    let Some(map) = map else {
        return Vec::new();
    };
    map.iter()
        .map(|(lang, value)| {
            let (ext, name) = match value {
                serde_json::Value::Array(arr) => {
                    let first = arr.first().and_then(|v| v.as_object());
                    (
                        first
                            .and_then(|o| o.get("ext"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        first
                            .and_then(|o| o.get("name"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                    )
                }
                _ => (None, None),
            };
            YtdlSubtitleTrack {
                language: lang.clone(),
                ext,
                name,
            }
        })
        .collect()
}

fn pick_recommended(
    formats: &[YtdlFormat],
    top_url: Option<&str>,
) -> (Option<String>, Option<String>) {
    // Prefer progressive (video+audio) around 720p, else best with both codecs, else top_url.
    let progressive: Vec<&YtdlFormat> = formats
        .iter()
        .filter(|f| {
            let v = f.vcodec.as_deref().unwrap_or("none");
            let a = f.acodec.as_deref().unwrap_or("none");
            v != "none" && a != "none" && f.url.is_some()
        })
        .collect();

    let pick = progressive
        .iter()
        .filter(|f| f.height.unwrap_or(0) <= 720)
        .max_by_key(|f| f.height.unwrap_or(0))
        .copied()
        .or_else(|| {
            progressive
                .iter()
                .max_by_key(|f| f.height.unwrap_or(0))
                .copied()
        })
        .or_else(|| {
            formats
                .iter()
                .filter(|f| f.url.is_some())
                .max_by_key(|f| f.height.unwrap_or(0))
        });

    if let Some(f) = pick {
        return (Some(f.format_id.clone()), f.url.clone());
    }
    (None, top_url.map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_fixture_chapters_and_formats() {
        let raw = r#"{
            "id": "abc",
            "title": "Demo",
            "duration": 125.5,
            "webpage_url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "extractor_key": "Youtube",
            "chapters": [
                {"start_time": 0, "end_time": 10.5, "title": "Intro"},
                {"start_time": 10.5, "title": "Main"}
            ],
            "formats": [
                {
                    "format_id": "18",
                    "ext": "mp4",
                    "height": 360,
                    "vcodec": "avc1",
                    "acodec": "mp4a",
                    "url": "https://example.com/a.mp4"
                },
                {
                    "format_id": "22",
                    "ext": "mp4",
                    "height": 720,
                    "vcodec": "avc1",
                    "acodec": "mp4a",
                    "url": "https://example.com/b.mp4"
                }
            ],
            "subtitles": {
                "en": [{"ext": "vtt", "name": "English"}]
            }
        }"#;
        let parsed: YtdlJson = serde_json::from_str(raw).unwrap();
        let result = map_result(parsed, "fallback");
        assert_eq!(result.media_id, "youtube:dQw4w9WgXcQ");
        assert_eq!(result.duration_ms, Some(125_500));
        assert_eq!(result.chapters.len(), 2);
        assert_eq!(result.chapters[0].title.as_deref(), Some("Intro"));
        assert_eq!(result.chapters[0].end_ms, Some(10_500));
        assert_eq!(result.recommended_format_id.as_deref(), Some("22"));
        assert_eq!(result.subtitles.len(), 1);
    }
}
