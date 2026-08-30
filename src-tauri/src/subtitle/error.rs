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
            "ffmpeg executable was not found",
            details.map(str::to_string),
        )
    }

    pub fn file_not_found(path: &str) -> Self {
        Self::new(
            SubtitleErrorCode::FileNotFound,
            "file not found or inaccessible",
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
            "no subtitle tracks found in media",
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
