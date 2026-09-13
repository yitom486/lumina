//! Pick stream URLs for a format_id (progressive or video + paired audio).

use serde::{Deserialize, Serialize};

use crate::ytdl::error::YtdlError;
use crate::ytdl::model::{YtdlFormat, YtdlResolveResult};

#[derive(Debug, Clone)]
pub struct YtdlPlayTarget {
    pub page_url: String,
    pub media_id: String,
    pub format_id: String,
    pub stream_url: String,
    pub audio_url: Option<String>,
    /// yt-dlp known duration; mpv often reports 0 until demux settles.
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackFormatOption {
    pub format_id: String,
    pub label: String,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackFormatsResponse {
    pub formats: Vec<PlaybackFormatOption>,
    pub current_format_id: Option<String>,
}

impl YtdlResolveResult {
    /// Strip signed CDN URLs before crossing to WebView / Agent.
    pub fn sanitized_for_ipc(&self) -> Self {
        let mut clone = self.clone();
        clone.recommended_url = None;
        for format in &mut clone.formats {
            format.url = None;
        }
        for subtitle in &mut clone.subtitles {
            subtitle.url = None;
        }
        clone
    }
}

pub fn play_target(
    resolved: &YtdlResolveResult,
    page_url: &str,
    format_id: Option<&str>,
) -> Result<YtdlPlayTarget, YtdlError> {
    let chosen_id = format_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(|| resolved.recommended_format_id.clone())
        .ok_or_else(|| YtdlError::invalid("没有可用的播放清晰度"))?;

    let format = resolved
        .formats
        .iter()
        .find(|f| f.format_id == chosen_id)
        .ok_or_else(|| YtdlError::invalid("所选清晰度不可用"))?;

    let stream_url = format
        .url
        .clone()
        .or_else(|| {
            if resolved.recommended_format_id.as_deref() == Some(chosen_id.as_str()) {
                resolved.recommended_url.clone()
            } else {
                None
            }
        })
        .ok_or_else(|| YtdlError::resolve_failed(Some("format url missing")))?;

    let audio_url = if has_audio(format) {
        None
    } else {
        pick_best_audio_url(&resolved.formats)
    };

    Ok(YtdlPlayTarget {
        page_url: page_url.to_string(),
        media_id: resolved.media_id.clone(),
        format_id: chosen_id,
        stream_url,
        audio_url,
        duration_ms: resolved.duration_ms,
    })
}

/// Progressive or video-only (pairable) options with a URL, highest first, one per height.
pub fn progressive_options(formats: &[YtdlFormat]) -> Vec<PlaybackFormatOption> {
    let has_audio_track = formats
        .iter()
        .any(|f| has_audio(f) && !has_video(f) && f.url.is_some());
    let mut candidates: Vec<&YtdlFormat> = formats
        .iter()
        .filter(|f| {
            if is_hls_url(f.url.as_deref()) || f.ext.as_deref() == Some("m3u8") {
                return false;
            }
            if !has_video(f) || f.url.as_ref().is_none_or(|u| u.is_empty()) {
                return false;
            }
            has_audio(f) || has_audio_track
        })
        .collect();

    candidates.sort_by(|a, b| {
        b.height
            .unwrap_or(0)
            .cmp(&a.height.unwrap_or(0))
            .then_with(|| {
                b.tbr
                    .unwrap_or(0.0)
                    .partial_cmp(&a.tbr.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut seen_heights = std::collections::HashSet::new();
    let mut out = Vec::new();
    for f in candidates {
        let height = f.height.unwrap_or(0);
        if height > 0 && !seen_heights.insert(height) {
            continue;
        }
        out.push(PlaybackFormatOption {
            format_id: f.format_id.clone(),
            label: format_label(f),
            height: f.height,
        });
    }
    out
}

fn format_label(f: &YtdlFormat) -> String {
    let height = f
        .height
        .map(|h| format!("{h}p"))
        .or_else(|| f.format_note.clone())
        .unwrap_or_else(|| f.format_id.clone());
    match f.ext.as_deref() {
        Some(ext) if !ext.is_empty() => format!("{height} · {ext}"),
        _ => height,
    }
}

fn has_audio(format: &YtdlFormat) -> bool {
    match format.acodec.as_deref() {
        None | Some("none") => false,
        Some(_) => true,
    }
}

fn has_video(format: &YtdlFormat) -> bool {
    match format.vcodec.as_deref() {
        None | Some("none") => false,
        Some(_) => true,
    }
}

fn is_hls_url(url: Option<&str>) -> bool {
    url.is_some_and(|u| {
        let lower = u.to_ascii_lowercase();
        lower.contains(".m3u8") || lower.contains("/manifest/hls")
    })
}

fn pick_best_audio_url(formats: &[YtdlFormat]) -> Option<String> {
    formats
        .iter()
        .filter(|f| has_audio(f) && !has_video(f) && f.url.is_some())
        .max_by(|a, b| {
            a.tbr
                .partial_cmp(&b.tbr)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|f| f.url.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(
        id: &str,
        height: Option<u32>,
        vcodec: &str,
        acodec: &str,
        url: &str,
        tbr: Option<f64>,
    ) -> YtdlFormat {
        YtdlFormat {
            format_id: id.into(),
            ext: Some("mp4".into()),
            height,
            width: None,
            fps: None,
            vcodec: Some(vcodec.into()),
            acodec: Some(acodec.into()),
            tbr,
            format_note: None,
            url: Some(url.into()),
        }
    }

    #[test]
    fn progressive_needs_no_audio_pair() {
        let resolved = YtdlResolveResult {
            media_id: "youtube:x".into(),
            title: None,
            duration_ms: None,
            webpage_url: Some("https://www.youtube.com/watch?v=x".into()),
            extractor: None,
            chapters: vec![],
            formats: vec![fmt(
                "22",
                Some(720),
                "avc1",
                "mp4a",
                "https://cdn/v.mp4",
                Some(1000.0),
            )],
            subtitles: vec![],
            recommended_url: Some("https://cdn/v.mp4".into()),
            recommended_format_id: Some("22".into()),
        };
        let target = play_target(&resolved, "https://www.youtube.com/watch?v=x", None).unwrap();
        assert_eq!(target.format_id, "22");
        assert!(target.audio_url.is_none());
        assert_eq!(target.stream_url, "https://cdn/v.mp4");
    }

    #[test]
    fn video_only_pairs_best_audio() {
        let resolved = YtdlResolveResult {
            media_id: "youtube:x".into(),
            title: None,
            duration_ms: None,
            webpage_url: None,
            extractor: None,
            chapters: vec![],
            formats: vec![
                fmt(
                    "137",
                    Some(1080),
                    "avc1",
                    "none",
                    "https://cdn/v.mp4",
                    Some(4000.0),
                ),
                fmt(
                    "140",
                    None,
                    "none",
                    "mp4a",
                    "https://cdn/a-lo.m4a",
                    Some(128.0),
                ),
                fmt(
                    "251",
                    None,
                    "none",
                    "opus",
                    "https://cdn/a-hi.webm",
                    Some(160.0),
                ),
            ],
            subtitles: vec![],
            recommended_url: None,
            recommended_format_id: Some("137".into()),
        };
        let target = play_target(&resolved, "https://yt/x", Some("137")).unwrap();
        assert_eq!(target.audio_url.as_deref(), Some("https://cdn/a-hi.webm"));
    }

    #[test]
    fn sanitized_strips_urls() {
        let resolved = YtdlResolveResult {
            media_id: "youtube:x".into(),
            title: None,
            duration_ms: None,
            webpage_url: None,
            extractor: None,
            chapters: vec![],
            formats: vec![fmt("22", Some(720), "avc1", "mp4a", "https://secret", None)],
            subtitles: vec![crate::ytdl::YtdlSubtitleTrack {
                language: "en".into(),
                ext: Some("vtt".into()),
                name: None,
                url: Some("https://secret/subtitle".into()),
            }],
            recommended_url: Some("https://secret".into()),
            recommended_format_id: Some("22".into()),
        };
        let clean = resolved.sanitized_for_ipc();
        assert!(clean.recommended_url.is_none());
        assert!(clean.formats[0].url.is_none());
        assert!(clean.subtitles[0].url.is_none());
    }

    #[test]
    fn progressive_dedupes_by_height() {
        let formats = vec![
            fmt(
                "18",
                Some(360),
                "avc1",
                "mp4a",
                "https://a/360.mp4",
                Some(360.0),
            ),
            fmt(
                "22",
                Some(720),
                "avc1",
                "mp4a",
                "https://a/720.mp4",
                Some(720.0),
            ),
            fmt(
                "22b",
                Some(720),
                "avc1",
                "mp4a",
                "https://a/720b.mp4",
                Some(700.0),
            ),
            fmt(
                "137",
                Some(1080),
                "avc1",
                "none",
                "https://a/v-only.mp4",
                None,
            ),
            fmt(
                "140",
                None,
                "none",
                "mp4a",
                "https://a/audio.m4a",
                Some(128.0),
            ),
        ];
        let opts = progressive_options(&formats);
        assert_eq!(opts.len(), 3);
        assert_eq!(opts[0].format_id, "137");
        assert_eq!(opts[1].format_id, "22");
        assert_eq!(opts[1].label, "720p · mp4");
        assert_eq!(opts[2].format_id, "18");
    }

    #[test]
    fn progressive_options_skips_hls_manifest() {
        let formats = vec![
            fmt(
                "95",
                Some(720),
                "avc1",
                "mp4a",
                "https://manifest.googlevideo.com/api/manifest/hls_playlist/x/playlist/index.m3u8",
                Some(2000.0),
            ),
            fmt(
                "22",
                Some(720),
                "avc1",
                "mp4a",
                "https://cdn/v.mp4",
                Some(1000.0),
            ),
        ];
        let opts = progressive_options(&formats);
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].format_id, "22");
    }
}
