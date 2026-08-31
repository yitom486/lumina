//! Structured ASR errors.
//! `message` = fixed business Chinese; tool/serde text only in `details` (+ tracing).

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AsrErrorCode {
    NotConfigured,
    Busy,
    ExtractFailed,
    TranscribeFailed,
    Cancelled,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrError {
    pub code: AsrErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl AsrError {
    pub fn new(code: AsrErrorCode, message: impl Into<String>, details: Option<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn not_configured(details: Option<&str>) -> Self {
        Self::new(
            AsrErrorCode::NotConfigured,
            "未配置语音转写（可选）",
            details
                .map(str::to_string)
                .or_else(|| Some("place whisper-cli + ggml model under native/whisper/".into())),
        )
    }

    pub fn busy() -> Self {
        Self::new(AsrErrorCode::Busy, "已有语音转写任务在运行", None)
    }

    pub fn extract_failed(details: Option<&str>) -> Self {
        Self::new(
            AsrErrorCode::ExtractFailed,
            "无法提取音频",
            details.map(str::to_string),
        )
    }

    pub fn transcribe_failed(details: Option<&str>) -> Self {
        Self::new(
            AsrErrorCode::TranscribeFailed,
            "语音转写失败",
            details.map(str::to_string),
        )
    }

    pub fn cancelled() -> Self {
        Self::new(AsrErrorCode::Cancelled, "已取消语音转写", None)
    }

    pub fn internal(details: Option<&str>) -> Self {
        Self::new(
            AsrErrorCode::InternalError,
            "内部错误，请重试",
            details.map(str::to_string),
        )
    }
}

impl fmt::Display for AsrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.details {
            Some(details) => write!(f, "{:?}: {} ({details})", self.code, self.message),
            None => write!(f, "{:?}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for AsrError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_cjk(s: &str) -> bool {
        s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn constructors_use_chinese_business_messages() {
        assert!(has_cjk(&AsrError::not_configured(None).message));
        assert!(!AsrError::not_configured(None).message.contains("whisper"));
        assert!(has_cjk(&AsrError::busy().message));
        assert_eq!(AsrError::busy().code, AsrErrorCode::Busy);
        let t = AsrError::transcribe_failed(Some("whisper-cli exit 1"));
        assert_eq!(t.message, "语音转写失败");
        assert_eq!(t.details.as_deref(), Some("whisper-cli exit 1"));
        assert_eq!(
            AsrError::not_configured(None).code,
            AsrErrorCode::NotConfigured
        );
    }
}
