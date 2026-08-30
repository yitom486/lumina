//! Structured media inspection errors.
//! `message` = business Chinese for UI; technical text only in `details` (+ tracing).

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MediaErrorCode {
    ProbeNotFound,
    FileNotFound,
    ProbeFailed,
    InvalidMedia,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaError {
    pub code: MediaErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl MediaError {
    pub fn new(
        code: MediaErrorCode,
        message: impl Into<String>,
        details: Option<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn probe_not_found(details: Option<&str>) -> Self {
        Self::new(
            MediaErrorCode::ProbeNotFound,
            "媒体分析组件未就绪",
            details
                .map(str::to_string)
                .or_else(|| Some("ffprobe missing under native/ffmpeg/".into())),
        )
    }

    pub fn file_not_found(path: &str) -> Self {
        Self::new(
            MediaErrorCode::FileNotFound,
            "找不到媒体文件或无法访问",
            Some(path.to_string()),
        )
    }

    /// User sees a stable business message; put tool/serde text in `details`.
    pub fn probe_failed(details: Option<&str>) -> Self {
        Self::new(
            MediaErrorCode::ProbeFailed,
            "无法读取该视频的媒体信息",
            details.map(str::to_string),
        )
    }

    pub fn invalid_media(details: Option<&str>) -> Self {
        Self::new(
            MediaErrorCode::InvalidMedia,
            "该文件无法作为媒体使用",
            details.map(str::to_string),
        )
    }

    pub fn internal(details: Option<&str>) -> Self {
        Self::new(
            MediaErrorCode::InternalError,
            "内部错误，请重试",
            details.map(str::to_string),
        )
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.details {
            Some(details) => write!(f, "{:?}: {} ({details})", self.code, self.message),
            None => write!(f, "{:?}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for MediaError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_cjk(s: &str) -> bool {
        s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn constructors_use_chinese_business_messages() {
        assert!(has_cjk(&MediaError::probe_not_found(None).message));
        assert!(!MediaError::probe_not_found(None).message.contains("ffprobe"));
        assert!(has_cjk(&MediaError::file_not_found("x").message));
        let probe = MediaError::probe_failed(Some("serde boom"));
        assert_eq!(probe.code, MediaErrorCode::ProbeFailed);
        assert_eq!(probe.message, "无法读取该视频的媒体信息");
        assert_eq!(probe.details.as_deref(), Some("serde boom"));
        assert!(!MediaError::invalid_media(Some("no format")).message.contains("format"));
    }
}
