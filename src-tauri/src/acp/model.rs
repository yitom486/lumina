//! ACP status / event / profile DTOs (camelCase for frontend).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::acp::profile::{AgentKind, AgentProfileStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSessionHint {
    pub session_id: String,
    pub profile_id: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpStatus {
    pub available: bool,
    pub adapter_found: bool,
    pub codex_found: bool,
    #[serde(default)]
    pub codex_config_found: bool,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfilesHint {
    pub active_profile_id: String,
    pub profiles: Vec<AgentProfileInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    },
    #[serde(rename_all = "camelCase")]
    ToolCallUpdate {
        tool_call_id: String,
        status: Option<String>,
        title: Option<String>,
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
