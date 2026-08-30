//! Structured subtitle / transcript errors.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SubtitleErrorCode {
    ToolNotFound,
    FileNotFound,
    ExtractFailed,
    ParseFailed,
    UnsupportedSubtitle,
    NoSubtitleTrack,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleError {
    pub code: SubtitleErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl SubtitleError {
    pub fn new(
        code: SubtitleErrorCode,
        message: impl Into<String>,
        details: Option<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn tool_not_found(details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::ToolNotFound,
            "未找到 ffmpeg，请放到 native/ffmpeg/",
            details.map(str::to_string),
        )
    }

    pub fn file_not_found(path: &str) -> Self {
        Self::new(
            SubtitleErrorCode::FileNotFound,
            "找不到字幕文件",
            Some(path.to_string()),
        )
    }

    pub fn extract_failed(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::ExtractFailed,
            message,
            details.map(str::to_string),
        )
    }

    pub fn parse_failed(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::ParseFailed,
            message,
            details.map(str::to_string),
        )
    }

    pub fn unsupported(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::UnsupportedSubtitle,
            message,
            details.map(str::to_string),
        )
    }

    pub fn no_track() -> Self {
        Self::new(
            SubtitleErrorCode::NoSubtitleTrack,
            "该媒体没有可用字幕轨",
            None,
        )
    }

    pub fn internal(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::InternalError,
            message,
            details.map(str::to_string),
        )
    }
}

impl fmt::Display for SubtitleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.details {
            Some(details) => write!(f, "{:?}: {} ({details})", self.code, self.message),
            None => write!(f, "{:?}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for SubtitleError {}

impl From<crate::media::MediaError> for SubtitleError {
    fn from(value: crate::media::MediaError) -> Self {
        use crate::media::MediaErrorCode;
        let code = match value.code {
            MediaErrorCode::ProbeNotFound => SubtitleErrorCode::ToolNotFound,
            MediaErrorCode::FileNotFound => SubtitleErrorCode::FileNotFound,
            MediaErrorCode::InvalidMedia => SubtitleErrorCode::UnsupportedSubtitle,
            _ => SubtitleErrorCode::ExtractFailed,
        };
        Self::new(code, value.message, value.details)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_cjk(s: &str) -> bool {
        s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn constructors_use_chinese_messages() {
        assert!(has_cjk(&SubtitleError::tool_not_found(None).message));
        assert!(has_cjk(&SubtitleError::file_not_found("x").message));
        assert!(has_cjk(&SubtitleError::no_track().message));
        assert_eq!(SubtitleError::no_track().code, SubtitleErrorCode::NoSubtitleTrack);
    }

    #[test]
    fn media_error_maps_codes() {
        let media = crate::media::MediaError::probe_not_found(None);
        let sub = SubtitleError::from(media);
        assert_eq!(sub.code, SubtitleErrorCode::ToolNotFound);
        assert!(has_cjk(&sub.message));
    }
}
