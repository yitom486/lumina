//! Structured ACP errors.
//! `message` = fixed business Chinese; spawn/protocol text only in `details` (+ tracing).
//! User-input validation may use `bad_request` with a specific Chinese `message`.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AcpErrorCode {
    NotConfigured,
    Busy,
    WorkspaceUnavailable,
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
    pub fn new(code: AcpErrorCode, message: impl Into<String>, details: Option<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn not_configured(details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::NotConfigured,
            "未配置 AI Agent（可选）",
            details.map(str::to_string).or_else(|| {
                Some("install Codex ACP adapter or configure another Agent command".into())
            }),
        )
    }

    pub fn busy() -> Self {
        Self::new(AcpErrorCode::Busy, "已有 AI 会话在运行", None)
    }

    pub fn workspace_unavailable(details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::WorkspaceUnavailable,
            "Agent 工作目录不可用，请重新打开视频后重试",
            details.map(str::to_string),
        )
    }

    pub fn spawn_failed(details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::SpawnFailed,
            "无法启动 AI Agent",
            details.map(str::to_string),
        )
    }

    pub fn codex_auth_required(details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::NotConfigured,
            "Codex 尚未登录或 API 未配置，请在终端运行 codex login 后重试",
            details.map(str::to_string),
        )
    }

    /// Wire / protocol / I/O failures against the Agent process.
    pub fn protocol(details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::ProtocolError,
            "与 Agent 通信失败",
            details.map(str::to_string),
        )
    }

    /// User-facing validation (empty prompt, bad profile fields, …).
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(AcpErrorCode::ProtocolError, message, None)
    }

    pub fn cancelled() -> Self {
        Self::new(AcpErrorCode::Cancelled, "已取消 AI 会话", None)
    }

    pub fn internal(details: Option<&str>) -> Self {
        Self::new(
            AcpErrorCode::InternalError,
            "内部错误，请重试",
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
    fn constructors_use_chinese_business_messages() {
        assert!(has_cjk(&AcpError::not_configured(None).message));
        assert!(!AcpError::not_configured(None).message.contains("Codex"));
        assert!(has_cjk(&AcpError::busy().message));
        assert!(has_cjk(&AcpError::cancelled().message));
        assert_eq!(AcpError::busy().code, AcpErrorCode::Busy);
        let workspace = AcpError::workspace_unavailable(Some("invalid cwd"));
        assert_eq!(workspace.code, AcpErrorCode::WorkspaceUnavailable);
        assert_eq!(
            workspace.message,
            "Agent 工作目录不可用，请重新打开视频后重试"
        );
        assert!(!workspace.message.contains("cwd"));
        let p = AcpError::protocol(Some("json parse failed"));
        assert_eq!(p.message, "与 Agent 通信失败");
        assert_eq!(p.details.as_deref(), Some("json parse failed"));
        assert_eq!(
            AcpError::bad_request("提问内容不能为空").message,
            "提问内容不能为空"
        );
        let auth = AcpError::codex_auth_required(Some("authenticate failed"));
        assert!(has_cjk(&auth.message));
        assert!(!auth.message.contains("authenticate"));
    }
}
