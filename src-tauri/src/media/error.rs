//! Structured media inspection errors.

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
            "ffprobe executable was not found",
            details.map(str::to_string),
        )
    }

    pub fn file_not_found(path: &str) -> Self {
        Self::new(
            MediaErrorCode::FileNotFound,
            "media file not found or inaccessible",
            Some(path.to_string()),
        )
    }

    pub fn probe_failed(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            MediaErrorCode::ProbeFailed,
            message,
            details.map(str::to_string),
        )
    }

    pub fn invalid_media(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            MediaErrorCode::InvalidMedia,
            message,
            details.map(str::to_string),
        )
    }

    pub fn internal(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            MediaErrorCode::InternalError,
            message,
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
