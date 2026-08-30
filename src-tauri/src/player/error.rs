//! Structured player errors. Frontend shape: `{ code, message, details? }`.
//! `message` is fixed user-facing Chinese; technical text goes in `details`.

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
            format!(
                "当前为「{}」状态，无法执行「{}」",
                status_label(status),
                operation_label(operation)
            ),
            None,
        )
    }

    pub fn playback(details: Option<&str>) -> Self {
        Self::new(
            PlayerErrorCode::PlaybackError,
            "播放操作失败，请重试",
            details.map(str::to_string),
        )
    }

    pub fn load(details: Option<&str>) -> Self {
        Self::new(
            PlayerErrorCode::LoadError,
            "无法打开该媒体文件",
            details.map(str::to_string),
        )
    }

    pub fn unsupported(details: Option<&str>) -> Self {
        Self::new(
            PlayerErrorCode::UnsupportedMedia,
            "不支持该媒体格式",
            details.map(str::to_string),
        )
    }

    pub fn native_window(details: Option<&str>) -> Self {
        Self::new(
            PlayerErrorCode::NativeWindowError,
            "视频窗口异常",
            details.map(str::to_string),
        )
    }

    pub fn initialization(details: Option<&str>) -> Self {
        Self::new(
            PlayerErrorCode::InitializationError,
            "播放引擎初始化失败",
            details.map(str::to_string),
        )
    }

    pub fn internal(details: Option<&str>) -> Self {
        Self::new(
            PlayerErrorCode::InternalError,
            "内部错误，请重试",
            details.map(str::to_string),
        )
    }

    pub fn backend_missing() -> Self {
        Self::internal(Some("libmpv backend is not attached"))
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

fn status_label(status: PlayerState) -> &'static str {
    match status {
        PlayerState::Idle => "空闲",
        PlayerState::Loading => "加载中",
        PlayerState::Ready => "就绪",
        PlayerState::Playing => "播放中",
        PlayerState::Paused => "已暂停",
        PlayerState::Ended => "已结束",
        PlayerState::Error => "错误",
    }
}

fn operation_label(operation: &str) -> &'static str {
    match operation {
        "open" => "打开",
        "play" => "播放",
        "pause" => "暂停",
        "stop" => "停止",
        "seek" => "跳转",
        _ => "该操作",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_messages_are_chinese() {
        let err = PlayerError::backend_missing();
        assert!(err.message.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        assert_eq!(err.code, PlayerErrorCode::InternalError);
        assert_eq!(err.message, "内部错误，请重试");

        let invalid = PlayerError::invalid_state("play", PlayerState::Error);
        assert!(invalid.message.contains("错误"));
        assert!(invalid.message.contains("播放"));
    }

    #[test]
    fn invalid_state_covers_common_ops() {
        for op in ["open", "pause", "stop", "seek"] {
            let err = PlayerError::invalid_state(op, PlayerState::Idle);
            assert_eq!(err.code, PlayerErrorCode::InvalidState);
            assert!(err.message.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        }
    }

    #[test]
    fn serde_shape_has_pascal_code() {
        let err = PlayerError::load(Some("raw"));
        let json = serde_json::to_value(&err).expect("json");
        assert_eq!(json["code"], "LoadError");
        assert_eq!(json["message"], "无法打开该媒体文件");
        assert_eq!(json["details"], "raw");
    }
}
