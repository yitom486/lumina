//! Structured player errors. Frontend shape: `{ code, message, details? }`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::player::model::PlayerState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PlayerErrorCode {
    InitializationError,
    LoadError,
    UnsupportedMedia,
    NativeWindowError,
    PlaybackError,
    InvalidState,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerError {
    pub code: PlayerErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl PlayerError {
    pub fn new(
        code: PlayerErrorCode,
        message: impl Into<String>,
        details: Option<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn invalid_state(operation: &str, status: PlayerState) -> Self {
        Self::new(
            PlayerErrorCode::InvalidState,
            format!("cannot {operation} while {status:?}"),
            None,
        )
    }

    pub fn playback(message: impl Into<String>) -> Self {
        Self::new(PlayerErrorCode::PlaybackError, message, None)
    }

    pub fn internal(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            PlayerErrorCode::InternalError,
            message,
            details.map(str::to_string),
        )
    }

    pub fn backend_missing() -> Self {
        Self::internal(
            "libmpv backend is not attached",
            Some("player runtime is not running"),
        )
    }
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.details {
            Some(details) => {
                write!(f, "{}: {} ({details})", format_code(self.code), self.message)
            }
            None => write!(f, "{}: {}", format_code(self.code), self.message),
        }
    }
}

impl std::error::Error for PlayerError {}

fn format_code(code: PlayerErrorCode) -> &'static str {
    match code {
        PlayerErrorCode::InitializationError => "InitializationError",
        PlayerErrorCode::LoadError => "LoadError",
        PlayerErrorCode::UnsupportedMedia => "UnsupportedMedia",
        PlayerErrorCode::NativeWindowError => "NativeWindowError",
        PlayerErrorCode::PlaybackError => "PlaybackError",
        PlayerErrorCode::InvalidState => "InvalidState",
        PlayerErrorCode::InternalError => "InternalError",
    }
}
