//! Media open target: local file or remote URL (online source).

use serde::{Deserialize, Serialize};

/// Machine-readable parse/validate failure. Domains map this to their own
/// fixed user-facing errors; see [`MediaSourceError::message`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaSourceErrorKind {
    Load,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSourceError {
    pub kind: MediaSourceErrorKind,
    pub details: Option<String>,
}

impl MediaSourceError {
    pub fn load(details: Option<&str>) -> Self {
        Self {
            kind: MediaSourceErrorKind::Load,
            details: details.map(str::to_string),
        }
    }

    pub fn unsupported(details: Option<&str>) -> Self {
        Self {
            kind: MediaSourceErrorKind::Unsupported,
            details: details.map(str::to_string),
        }
    }

    /// Stable user-facing string for this failure. Matches the player domain
    /// constructors (`LoadError` / `UnsupportedMedia`) by construction.
    pub fn message(&self) -> &'static str {
        match self.kind {
            MediaSourceErrorKind::Load => "无法打开该媒体文件",
            MediaSourceErrorKind::Unsupported => "不支持该媒体格式",
        }
    }
}

/// What the user asked to open. Playback target may later be a resolved stream URL
/// (yt-dlp); `media_id` stays stable for notes / history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MediaSource {
    Local { path: String },
    Remote { url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaSourceKind {
    Local,
    Remote,
}

impl MediaSource {
    /// Parse a user-facing path or `http(s)` URL.
    pub fn parse(input: &str) -> Result<Self, MediaSourceError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(MediaSourceError::load(Some("empty media path")));
        }

        if is_remote_url(trimmed) {
            return Ok(Self::Remote {
                url: trimmed.to_string(),
            });
        }

        Ok(Self::Local {
            path: trimmed.to_string(),
        })
    }

    pub fn kind(&self) -> MediaSourceKind {
        match self {
            Self::Local { .. } => MediaSourceKind::Local,
            Self::Remote { .. } => MediaSourceKind::Remote,
        }
    }

    /// String passed to libmpv `loadfile` (path or URL).
    pub fn playback_target(&self) -> &str {
        match self {
            Self::Local { path } => path.as_str(),
            Self::Remote { url } => url.as_str(),
        }
    }

    /// Stable id for notes / session. Remote uses scheme-host-path when possible;
    /// YouTube / Bilibili canonical forms land in a later step.
    pub fn media_id(&self) -> String {
        match self {
            Self::Local { path } => path.clone(),
            Self::Remote { url } => canonicalize_remote_id(url),
        }
    }

    pub fn validate(&self) -> Result<(), MediaSourceError> {
        match self {
            Self::Local { path } => validate_local_file(path),
            Self::Remote { url } => validate_remote_url(url),
        }
    }
}

fn is_remote_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

fn validate_local_file(path: &str) -> Result<(), MediaSourceError> {
    let meta = std::fs::metadata(path)
        .map_err(|error| MediaSourceError::load(Some(&format!("path not accessible: {error}"))))?;

    if !meta.is_file() {
        return Err(MediaSourceError::load(Some(&format!(
            "not a regular file: {path}"
        ))));
    }

    if meta.len() == 0 {
        return Err(MediaSourceError::unsupported(Some(&format!(
            "empty file: {path}"
        ))));
    }

    Ok(())
}

fn validate_remote_url(url: &str) -> Result<(), MediaSourceError> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(MediaSourceError::load(Some("remote url must be http(s)")));
    }
    // Host required (reject "https://")
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or("");
    if rest.is_empty() || rest.starts_with('/') {
        return Err(MediaSourceError::load(Some("remote url missing host")));
    }
    Ok(())
}

/// Best-effort stable id. Prefer platform ids when recognizable.
fn canonicalize_remote_id(url: &str) -> String {
    if let Some(id) = youtube_video_id(url) {
        return format!("youtube:{id}");
    }
    if let Some(id) = bilibili_video_id(url) {
        return format!("bilibili:{id}");
    }
    // Strip query/fragment for a stabler key (signed CDN URLs still change — Step 2 resolves).
    let without_frag = url.split('#').next().unwrap_or(url);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);
    without_query.to_string()
}

fn youtube_video_id(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.contains("youtube.com") || lower.contains("youtu.be")) {
        return None;
    }
    if let Some(rest) = url.split("youtu.be/").nth(1) {
        let id = rest.split(['?', '&', '/']).next().unwrap_or("");
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    for key in ["v=", "vi="] {
        if let Some(idx) = url.find(key) {
            let rest = &url[idx + key.len()..];
            let id = rest.split(['&', '#', '/']).next().unwrap_or("");
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    if let Some(rest) = url.split("/shorts/").nth(1) {
        let id = rest.split(['?', '&', '/']).next().unwrap_or("");
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

fn bilibili_video_id(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    if !lower.contains("bilibili.com") && !lower.contains("b23.tv") {
        return None;
    }
    for marker in ["/video/", "/bangumi/play/"] {
        if let Some(rest) = url.split(marker).nth(1) {
            let id = rest.split(['?', '&', '/']).next().unwrap_or("");
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_local_and_remote() {
        let local = MediaSource::parse(r"C:\a\b.mp4").unwrap();
        assert_eq!(local.kind(), MediaSourceKind::Local);
        assert_eq!(local.playback_target(), r"C:\a\b.mp4");

        let remote = MediaSource::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(remote.kind(), MediaSourceKind::Remote);
        assert_eq!(remote.media_id(), "youtube:dQw4w9WgXcQ");
    }

    #[test]
    fn bilibili_bv_id() {
        let src = MediaSource::parse("https://www.bilibili.com/video/BV1xx411c7mD?p=1").unwrap();
        assert_eq!(src.media_id(), "bilibili:BV1xx411c7mD");
    }

    #[test]
    fn remote_validation_rejects_empty_host() {
        let err = MediaSource::parse("https://")
            .unwrap()
            .validate()
            .unwrap_err();
        assert_eq!(err.kind, MediaSourceErrorKind::Load);
        assert!(err.details.as_deref().is_some_and(|d| d.contains("host")));
    }

    #[test]
    fn empty_input_is_load_error() {
        let err = MediaSource::parse("  ").unwrap_err();
        assert_eq!(err.kind, MediaSourceErrorKind::Load);
        assert_eq!(err.message(), "无法打开该媒体文件");
        assert_eq!(err.details.as_deref(), Some("empty media path"));
    }
}
