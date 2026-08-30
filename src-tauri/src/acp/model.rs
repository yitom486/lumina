//! ACP status / event DTOs (camelCase for frontend).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpStatus {
    pub available: bool,
    pub cli_path: Option<String>,
    pub codex_path: Option<String>,
    pub message: String,
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
