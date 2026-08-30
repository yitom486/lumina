//! ASR status / progress models.

use serde::{Deserialize, Serialize};

use crate::subtitle::Transcript;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrStatus {
    /// Engine binaries + model are present; still not loaded until first job.
    pub available: bool,
    pub cli_path: Option<String>,
    pub model_path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "PascalCase",
    rename_all_fields = "camelCase",
    tag = "type",
    content = "payload"
)]
pub enum AsrEvent {
    Started { path: String },
    Progress { stage: String, message: String },
    Finished { transcript: Transcript },
    Failed { code: String, message: String },
}
