//! Structured ACP errors.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AcpErrorCode {
    NotConfigured,
    Busy,
    SpawnFailed,
    ProtocolError,
    Cancelled,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpError {
    pub code: AcpErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl AcpError {
    pub fn new(
        code: AcpErrorCode,
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
            AcpErrorCode::NotConfigured,
            "未配置 ACP（可选）。请安装 Codex 与 codex-acp，并确保可在 PATH 中找到",
            details.map(str::to_string),
        )
    }

    pub fn busy() -> Self {
        Self::new(AcpErrorCode::Busy, "已有 ACP 会话在运行", None)
    }

    pub fn spawn_failed(details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::SpawnFailed,
            "无法启动 ACP 进程",
            details.map(str::to_string),
        )
    }

    pub fn protocol(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::ProtocolError,
            message,
            details.map(str::to_string),
        )
    }

    pub fn cancelled() -> Self {
        Self::new(AcpErrorCode::Cancelled, "已取消 ACP 会话", None)
    }

    pub fn internal(message: impl Into<String>, details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::InternalError,
            message,
            details.map(str::to_string),
        )
    }
}

impl fmt::Display for AcpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.details {
            Some(details) => write!(f, "{:?}: {} ({details})", self.code, self.message),
            None => write!(f, "{:?}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for AcpError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_cjk(s: &str) -> bool {
        s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    }

    #[test]
    fn constructors_use_chinese_messages() {
        assert!(has_cjk(&AcpError::not_configured(None).message));
        assert!(has_cjk(&AcpError::busy().message));
        assert!(has_cjk(&AcpError::cancelled().message));
        assert_eq!(AcpError::busy().code, AcpErrorCode::Busy);
    }
}
