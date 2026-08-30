//! Structured ASR errors.

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
    pub fn new(
        code: AsrErrorCode,
        message: impl Into<String>,
        details: Option<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn not_configured(details: Option<&str>) -> Self {
        Self::new(
            AsrErrorCode::NotConfigured,
            "ASR is not configured (optional). Place whisper-cli and a ggml model under native/whisper/",
            details.map(str::to_string),
        )
    }

    pub fn busy() -> Self {
        Self::new(AsrErrorCode::Busy, "an ASR job is already running", None)
    }

    pub fn extract_failed(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            AsrErrorCode::ExtractFailed,
            message,
            details.map(str::to_string),
        )
    }

    pub fn transcribe_failed(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            AsrErrorCode::TranscribeFailed,
            message,
            details.map(str::to_string),
        )
    }

    pub fn internal(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            AsrErrorCode::InternalError,
            message,
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
