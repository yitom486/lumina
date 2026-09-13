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

/// Durable frame reference attached to a note (P7-M3). `file` is a bare
/// filename under the notes `note-frames/` dir; export writes time+text only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NoteFrame {
    pub at_ms: u64,
    pub file: String,
}

/// JPEG bytes for one note frame thumbnail (lazy `notes_get_frame`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteFrameData {
    pub mime: String,
    pub data: String,
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
    #[serde(default)]
    pub frames: Vec<NoteFrame>,
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
    /// Capture one frame at `position_ms` into durable storage (best-effort).
    #[serde(default)]
    pub include_frame: Option<bool>,
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
