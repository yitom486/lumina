//! ASR status / progress models.

use serde::{Deserialize, Serialize};

use crate::subtitle::Transcript;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrModelInfo {
    pub id: String,
    pub path: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrStatus {
    /// Engine binaries + model are present; still not loaded until first job.
    pub available: bool,
    pub cli_path: Option<String>,
    pub model_path: Option<String>,
    #[serde(default)]
    pub models: Vec<AsrModelInfo>,
    pub message: String,
}

/// Optional transcription window. Omit / `None` = whole media file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AsrRange {
    Window { from_ms: u64, to_ms: u64 },
    Chapter { chapter_id: u32 },
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
