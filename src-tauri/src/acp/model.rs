//! ACP status / event / profile DTOs (camelCase for frontend).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::acp::profile::{AgentKind, AgentProfileStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpStatus {
    pub available: bool,
    /// Whether a Codex ACP adapter was found on disk/PATH (informational).
    pub adapter_found: bool,
    /// Whether a Codex binary was found (informational; adapter may bundle its own).
    pub codex_found: bool,
    pub active_profile_id: String,
    pub profiles: Vec<AgentProfileStatus>,
    pub cli_path: Option<String>,
    pub codex_path: Option<String>,
    pub message: String,
    pub hint: String,
    pub responses_only_note: String,
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
    Finished {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Failed {
        code: String,
        message: String,
    },
}
