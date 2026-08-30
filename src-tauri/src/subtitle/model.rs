//! Transcript / cue models.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SubtitleSource {
    Embedded,
    Sidecar,
}

/// One row in the subtitle dropdown — embedded track or auto-discovered sidecar file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleChoice {
    /// Stable id: `embedded:{streamIndex}` or `sidecar:{absolutePath}`.
    pub id: String,
    pub source: SubtitleSource,
    pub label: String,
    /// False for bitmap codecs (PGS etc.) — shown disabled in UI.
    pub supported: bool,
    pub stream_index: Option<u32>,
    pub external_path: Option<String>,
    pub codec_name: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub source_path: String,
    pub choice_id: String,
    pub stream_index: Option<u32>,
    pub language: Option<String>,
    pub codec_name: Option<String>,
    pub cues: Vec<Cue>,
}
