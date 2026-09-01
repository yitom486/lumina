//! Errors for the local media library index.
//! User-facing messages remain stable; filesystem details stay in `details`.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LibraryErrorCode {
    InvalidDirectory,
    InvalidInput,
    GroupNotFound,
    PrivacyConsentRequired,
    CredentialAccessFailed,
    ResolverNotConfigured,
    RemoteRequestFailed,
    InvalidResolverResponse,
    ScanFailed,
    StorageFailed,
    NotRunning,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryError {
    pub code: LibraryErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl LibraryError {
    fn new(code: LibraryErrorCode, message: &'static str, details: Option<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn invalid_directory(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::InvalidDirectory,
            "媒体目录不存在或无法访问",
            details.map(str::to_string),
        )
    }

    pub fn scan_failed(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::ScanFailed,
            "媒体目录扫描失败，请重试",
            details.map(str::to_string),
        )
    }

    pub fn invalid_input(message: &'static str) -> Self {
        Self::new(LibraryErrorCode::InvalidInput, message, None)
    }

    pub fn group_not_found(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::GroupNotFound,
            "找不到待处理的媒体分组",
            details.map(str::to_string),
        )
    }

    pub fn privacy_consent_required() -> Self {
        Self::new(
            LibraryErrorCode::PrivacyConsentRequired,
            "请先确认允许发送文件名用于智能匹配",
            None,
        )
    }

    pub fn credential_access_failed(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::CredentialAccessFailed,
            "无法访问系统安全凭据，请检查系统账户后重试",
            details.map(str::to_string),
        )
    }

    pub fn resolver_not_configured(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::ResolverNotConfigured,
            "未配置媒体智能匹配服务",
            details.map(str::to_string),
        )
    }

    pub fn remote_request_failed(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::RemoteRequestFailed,
            "媒体信息查询失败，请稍后重试",
            details.map(str::to_string),
        )
    }

    pub fn invalid_resolver_response(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::InvalidResolverResponse,
            "智能匹配结果无效，请改用手动标题",
            details.map(str::to_string),
        )
    }

    pub fn storage_failed(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::StorageFailed,
            "媒体索引保存失败，请重试",
            details.map(str::to_string),
        )
    }

    pub fn not_running() -> Self {
        Self::new(LibraryErrorCode::NotRunning, "媒体目录守护服务未启动", None)
    }

    pub fn internal(details: Option<&str>) -> Self {
        Self::new(
            LibraryErrorCode::InternalError,
            "内部错误，请重试",
            details.map(str::to_string),
        )
    }
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.details {
            Some(details) => write!(f, "{:?}: {} ({details})", self.code, self.message),
            None => write!(f, "{:?}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for LibraryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_business_facing() {
        let error = LibraryError::scan_failed(Some("Permission denied"));
        assert_eq!(error.message, "媒体目录扫描失败，请重试");
        assert_eq!(error.details.as_deref(), Some("Permission denied"));
        assert_eq!(
            LibraryError::not_running().message,
            "媒体目录守护服务未启动"
        );
    }
}
