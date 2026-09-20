//! ACP status / event / profile DTOs (camelCase for frontend).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Pure agent-kind discriminant. Owned by `domain` so profile DTOs never
/// pull in launch/discovery logic; `agent` re-exports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AgentKind {
    Codex,
    Claude,
    Antigravity,
    Custom,
}

/// Optional per-profile behavior presets. Absent everywhere = fully generic
/// ACP: plain `command + args` spawn, no env injection, first advertised auth
/// method, no agent-side session cleanup. These keep host conveniences as
/// profile *data* instead of `AgentKind` code branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LauncherPreset {
    /// codex-acp fallback chain: standalone adapter → dev tree → `bun x` → `bunx`.
    #[serde(rename = "codex-acp")]
    CodexAcp,
    /// `command` → antigravity-acp discovery fallback (`find_antigravity`).
    #[serde(rename = "antigravity-acp")]
    AntigravityAcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvPreset {
    /// Inject `CODEX_PATH` / `CODEX_HOME` / `TERM` and prepend the Codex bin dir.
    #[serde(rename = "codex-cli")]
    CodexCli,
    /// Inject local HTTP/SOCKS proxy env (`ACP_PROXY_PORT`, default 7897).
    #[serde(rename = "antigravity-proxy")]
    AntigravityProxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthPolicy {
    /// Codex-style dynamic auth preference (ChatGPT login vs API key).
    #[serde(rename = "codex-local")]
    CodexLocal,
    /// Antigravity-style dynamic auth preference (Google OAuth vs Gemini key).
    #[serde(rename = "antigravity-oauth")]
    AntigravityOauth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStoragePreset {
    /// Sessions persist as `~/.codex/sessions` rollouts (pool-close cleanup).
    #[serde(rename = "codex-rollouts")]
    CodexRollouts,
}

/// Explicit Lumina-owned session purpose stored in ACP session metadata.
/// This must be passed by each creation path; it is never inferred from
/// tool access or other mutable service state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Chat,
    Workshop,
    Chapter,
}

impl SessionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Workshop => "workshop",
            Self::Chapter => "chapter",
        }
    }
}

/// What happened to the Agent memory the user asked to restore.
///
/// `Occupied` and `Unavailable` must stay distinct: an occupied conversation
/// is fully intact and becomes restorable again once whatever holds it lets
/// go, whereas an unavailable one is gone for good. Collapsing them told the
/// user their memory no longer existed when it actually did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResumeOutcome {
    Resumed,
    Occupied,
    Unavailable,
}

/// Per-profile availability snapshot for the frontend. Pure DTO:
/// resolution itself lives in `agent::profile` technique.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileStatus {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub available: bool,
    pub resolved_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSessionHint {
    pub session_id: String,
    pub profile_id: String,
    pub cwd: String,
}

/// User-pasted image riding a prompt (`session/prompt` image block).
/// `data` is raw base64 (no `data:` prefix); the wire layer wraps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptImage {
    pub mime_type: String,
    pub data: String,
}

/// Pasted-image guardrails: user input validation, so failures speak
/// plain business Chinese (`bad_request`) instead of leaking wire terms.
pub fn validate_prompt_images(images: &[PromptImage]) -> Result<(), crate::error::AcpError> {
    const ALLOWED: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];
    /// Base64 chars per image (~6MB decoded).
    const MAX_IMAGE_CHARS: usize = 8 * 1024 * 1024;
    const MAX_IMAGES: usize = 4;
    if images.len() > MAX_IMAGES {
        return Err(crate::error::AcpError::bad_request("一次最多发送 4 张图片"));
    }
    for image in images {
        if !ALLOWED.contains(&image.mime_type.trim().to_ascii_lowercase().as_str()) {
            return Err(crate::error::AcpError::bad_request(
                "图片格式不支持，仅支持 PNG/JPEG/WebP/GIF",
            ));
        }
        if image.data.trim().is_empty() {
            return Err(crate::error::AcpError::bad_request("图片内容为空"));
        }
        if image.data.len() > MAX_IMAGE_CHARS {
            return Err(crate::error::AcpError::bad_request(
                "图片过大，单张不能超过 8MB",
            ));
        }
    }
    Ok(())
}

/// Metadata returned by an Agent's session listing. Message content is never
/// part of this DTO. The kind is recorded for future filtering, but is not
/// used as a filter in this batch because existing sessions are unmarked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionInfo {
    pub session_id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub updated_at: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionListResult {
    pub verified: bool,
    pub sessions: Vec<AgentSessionInfo>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpStatus {
    pub available: bool,
    pub adapter_found: bool,
    pub codex_found: bool,
    #[serde(default)]
    pub codex_config_found: bool,
    #[serde(default)]
    pub antigravity_found: bool,
    #[serde(default)]
    pub antigravity_credentials_found: bool,
    pub active_profile_id: String,
    pub profiles: Vec<AgentProfileStatus>,
    pub cli_path: Option<String>,
    pub codex_path: Option<String>,
    pub message: String,
    pub hint: String,
    pub responses_only_note: String,
    /// Whether a live Agent process/session is currently open.
    pub session_active: bool,
    pub busy: bool,
    /// Model / reasoning options from the live session, when connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_model_options: Option<AcpSessionModelOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfilesHint {
    pub active_profile_id: String,
    pub profiles: Vec<AgentProfileInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileInput {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<LauncherPreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_preset: Option<EnvPreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_policy: Option<AuthPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth_methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_storage: Option<SessionStoragePreset>,
}

/// A selectable string-valued ACP session configuration option.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionOption {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}

/// The model-related options reported after an ACP session is opened.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionModelOptions {
    pub models: Vec<AcpSessionOption>,
    pub reasoning_efforts: Vec<AcpSessionOption>,
    pub current_model_id: Option<String>,
    pub current_reasoning_effort: Option<String>,
}

/// A model override applied to one newly-created ACP session before its first
/// prompt. It is never persisted as a chat-session mutation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpSessionModelSelection {
    pub model_id: String,
    pub reasoning_effort: Option<String>,
}

/// Explicit, short-lived connection result for media matching. It deliberately
/// contains no credentials and no chat-session history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpModelDiscoveryResult {
    pub connected: bool,
    pub options: AcpSessionModelOptions,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AcpEvent {
    Started,
    #[serde(rename_all = "camelCase")]
    Progress {
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    AgentMessage {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    AgentThought {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    ToolCall {
        tool_call_id: String,
        title: Option<String>,
        kind: Option<String>,
        status: Option<String>,
        detail: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ToolCallUpdate {
        tool_call_id: String,
        status: Option<String>,
        title: Option<String>,
        detail: Option<String>,
        #[serde(default)]
        append_detail: bool,
    },
    #[serde(rename_all = "camelCase")]
    Plan {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    SessionSaved {
        session_id: String,
        profile_id: String,
        cwd: String,
        /// `None` when no restore was attempted (a plain new conversation).
        resume: Option<ResumeOutcome>,
    },
    #[serde(rename_all = "camelCase")]
    PermissionRequest {
        request_id: String,
        tool_call_id: Option<String>,
        title: Option<String>,
        options: Vec<PermissionOption>,
    },
    #[serde(rename_all = "camelCase")]
    PermissionResolved {
        tool_call_id: Option<String>,
        decision: String,
    },
    #[serde(rename_all = "camelCase")]
    Finished {
        text: String,
        stop_reason: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Failed {
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(mime_type: &str, data: &str) -> PromptImage {
        PromptImage {
            mime_type: mime_type.to_string(),
            data: data.to_string(),
        }
    }

    #[test]
    fn prompt_image_validation_accepts_supported_kinds() {
        for mime in ["image/png", "image/jpeg", "image/webp", "image/gif"] {
            validate_prompt_images(std::slice::from_ref(&image(mime, "aGVsbG8=")))
                .expect("supported image");
        }
        validate_prompt_images(&[]).expect("no images");
    }

    #[test]
    fn prompt_image_validation_rejects_with_business_chinese() {
        let bad_mime =
            validate_prompt_images(std::slice::from_ref(&image("image/svg+xml", "aGVsbG8=")))
                .expect_err("svg rejected");
        assert!(bad_mime.message.contains("PNG"));

        let empty = validate_prompt_images(std::slice::from_ref(&image("image/png", "  ")))
            .expect_err("empty rejected");
        assert!(empty.message.contains("为空"));

        let oversized = validate_prompt_images(std::slice::from_ref(&image(
            "image/png",
            &"a".repeat(8 * 1024 * 1024 + 1),
        )))
        .expect_err("oversized rejected");
        assert!(oversized.message.contains("8MB"));

        let too_many: Vec<PromptImage> = (0..5).map(|_| image("image/png", "aGVsbG8=")).collect();
        let capped = validate_prompt_images(&too_many).expect_err("fifth image rejected");
        assert!(capped.message.contains("4 张"));
    }

    #[test]
    fn session_kind_serializes_to_stable_lowercase_strings() {
        assert_eq!(
            serde_json::to_string(&SessionKind::Chat).unwrap(),
            "\"chat\""
        );
        assert_eq!(
            serde_json::to_string(&SessionKind::Workshop).unwrap(),
            "\"workshop\""
        );
        assert_eq!(
            serde_json::to_string(&SessionKind::Chapter).unwrap(),
            "\"chapter\""
        );
        assert_eq!(SessionKind::Chapter.as_str(), "chapter");
        assert_eq!(
            serde_json::from_str::<SessionKind>("\"chapter\"").unwrap(),
            SessionKind::Chapter
        );
    }
}
