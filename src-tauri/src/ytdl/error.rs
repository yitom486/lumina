//! Structured yt-dlp errors. `message` = business Chinese; tools only in `details`.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum YtdlErrorCode {
    NotConfigured,
    Busy,
    InvalidRequest,
    ResolveFailed,
    DownloadFailed,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YtdlError {
    pub code: YtdlErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl YtdlError {
    pub fn new(code: YtdlErrorCode, message: impl Into<String>, details: Option<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn not_configured(details: Option<&str>) -> Self {
        Self::new(
            YtdlErrorCode::NotConfigured,
            "未配置在线视频解析（可选）",
            details.map(str::to_string),
        )
    }

    pub fn busy() -> Self {
        Self::new(YtdlErrorCode::Busy, "已有在线解析任务在运行", None)
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(YtdlErrorCode::InvalidRequest, message, None)
    }

    pub fn resolve_failed(details: Option<&str>) -> Self {
        Self::new(
            YtdlErrorCode::ResolveFailed,
            "无法解析该在线视频",
            details.map(str::to_string),
        )
    }

    pub fn download_failed(details: Option<&str>) -> Self {
        Self::new(
            YtdlErrorCode::DownloadFailed,
            "下载在线解析组件失败，请检查网络后重试",
            details.map(str::to_string),
        )
    }

    pub fn internal(details: Option<&str>) -> Self {
        Self::new(
            YtdlErrorCode::InternalError,
            "内部错误，请重试",
            details.map(str::to_string),
        )
    }
}

impl fmt::Display for YtdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for YtdlError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_use_chinese_business_messages() {
        let e = YtdlError::not_configured(Some("missing yt-dlp.exe"));
        assert!(e
            .message
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        assert!(!e.message.to_ascii_lowercase().contains("yt-dlp"));
        assert!(e.details.as_deref().is_some_and(|d| d.contains("yt-dlp")));

        let d = YtdlError::download_failed(Some("HTTP 404"));
        assert!(!d.message.contains("HTTP"));
        assert_eq!(d.code, YtdlErrorCode::DownloadFailed);
    }
}
