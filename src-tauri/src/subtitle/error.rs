//! Structured subtitle / transcript errors.
//! `message` = fixed business Chinese; tool/serde text only in `details` (+ tracing).

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
    ExportFailed,
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
            "字幕工具未就绪",
            details
                .map(str::to_string)
                .or_else(|| Some("ffmpeg missing under native/ffmpeg/".into())),
        )
    }

    pub fn file_not_found(path: &str) -> Self {
        Self::new(
            SubtitleErrorCode::FileNotFound,
            "找不到字幕文件",
            Some(path.to_string()),
        )
    }

    pub fn extract_failed(details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::ExtractFailed,
            "无法提取字幕",
            details.map(str::to_string),
        )
    }

    pub fn parse_failed(details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::ParseFailed,
            "无法解析字幕",
            details.map(str::to_string),
        )
    }

    pub fn unsupported(details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::UnsupportedSubtitle,
            "不支持该字幕格式",
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

    pub fn export_failed(details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::ExportFailed,
            "无法保存字幕文件",
            details.map(str::to_string),
        )
    }

    pub fn internal(details: Option<&str>) -> Self {
        Self::new(
            SubtitleErrorCode::InternalError,
            "内部错误，请重试",
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
        let details = value.details.as_deref();
        match value.code {
            MediaErrorCode::ProbeNotFound => Self::tool_not_found(details),
            MediaErrorCode::FileNotFound => Self::file_not_found(details.unwrap_or("unknown path")),
            MediaErrorCode::InvalidMedia => Self::unsupported(details),
            _ => Self::extract_failed(details),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_cjk(s: &str) -> bool {
        s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn constructors_use_chinese_business_messages() {
        assert!(has_cjk(&SubtitleError::tool_not_found(None).message));
        assert!(!SubtitleError::tool_not_found(None)
            .message
            .contains("ffmpeg"));
        assert!(has_cjk(&SubtitleError::file_not_found("x").message));
        assert!(has_cjk(&SubtitleError::no_track().message));
        assert_eq!(
            SubtitleError::export_failed(Some("io")).message,
            "无法保存字幕文件"
        );
        let extract = SubtitleError::extract_failed(Some("ffmpeg spawn failed"));
        assert_eq!(extract.message, "无法提取字幕");
        assert_eq!(extract.details.as_deref(), Some("ffmpeg spawn failed"));
        assert_eq!(
            SubtitleError::no_track().code,
            SubtitleErrorCode::NoSubtitleTrack
        );
    }

    #[test]
    fn media_error_maps_to_subtitle_business_messages() {
        let media = crate::media::MediaError::probe_not_found(None);
        let sub = SubtitleError::from(media);
        assert_eq!(sub.code, SubtitleErrorCode::ToolNotFound);
        assert_eq!(sub.message, "字幕工具未就绪");
    }
}
