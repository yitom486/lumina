//! Structured note errors.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum NoteErrorCode {
    IoError,
    NotFound,
    InvalidNote,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteError {
    pub code: NoteErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl NoteError {
    pub fn new(
        code: NoteErrorCode,
        message: impl Into<String>,
        details: Option<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn io(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            NoteErrorCode::IoError,
            message,
            details.map(str::to_string),
        )
    }

    pub fn not_found(id: &str) -> Self {
        Self::new(
            NoteErrorCode::NotFound,
            "找不到该笔记",
            Some(id.to_string()),
        )
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(NoteErrorCode::InvalidNote, message, None)
    }

    pub fn internal(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            NoteErrorCode::InternalError,
            message,
            details.map(str::to_string),
        )
    }
}

impl fmt::Display for NoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.details {
            Some(details) => write!(f, "{:?}: {} ({details})", self.code, self.message),
            None => write!(f, "{:?}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for NoteError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_chinese() {
        assert!(NoteError::not_found("x")
            .message
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        assert!(NoteError::invalid("内容不能为空")
            .message
            .contains("空"));
    }
}
