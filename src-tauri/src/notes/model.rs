//! Note DTOs (camelCase for frontend).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NoteQuote {
    pub index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub anchor: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub media_path: String,
    pub position_ms: u64,
    pub body: String,
    #[serde(default)]
    pub quotes: Vec<NoteQuote>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteCreate {
    pub media_path: String,
    pub position_ms: u64,
    pub body: String,
    #[serde(default)]
    pub subtitle_choice_id: Option<String>,
    #[serde(default)]
    pub anchor_cue_index: Option<u32>,
    #[serde(default)]
    pub quote_cue_indices: Option<Vec<u32>>,
    #[serde(default)]
    pub quote_hint: Option<String>,
    #[serde(default)]
    pub include_quotes: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotePreviewQuotes {
    pub media_path: String,
    pub position_ms: u64,
    #[serde(default)]
    pub subtitle_choice_id: Option<String>,
    #[serde(default)]
    pub anchor_cue_index: Option<u32>,
    #[serde(default)]
    pub quote_cue_indices: Option<Vec<u32>>,
    #[serde(default)]
    pub quote_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteUpdate {
    pub id: String,
    pub body: Option<String>,
    pub position_ms: Option<u64>,
}
