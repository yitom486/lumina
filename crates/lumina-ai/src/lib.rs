//! Agent-backed subtitle tasks (library crate).
//!
//! Translation runs as short-lived, data-isolated tasks through
//! [`AgentInvoker`]; Chat history and general MCP tools are never involved.

pub mod chapter;
pub mod prompts {
    //! Typed evidence contracts used by chapter prompts.

    use lumina_subtitle::Cue;
    use serde::{Deserialize, Serialize};

    /// A fixed-width transcript evidence block.
    ///
    /// This is an evidence grouping, not a semantic or mechanical chapter
    /// boundary. A cue crossing a bucket boundary may therefore occur in
    /// both adjacent windows.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct TranscriptWindow {
        pub window_id: String,
        pub start_ms: u64,
        pub end_ms: u64,
        pub cues: Vec<Cue>,
    }

    /// A reference to a previously generated screenshot asset.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ScreenshotReference {
        pub asset_id: String,
        pub timestamp_ms: u64,
        pub resource_ref: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub note: Option<String>,
    }
}

pub mod translate;

pub use lumina_core::{AgentInvoker, AgentTaskError, IsolatedAgentTask};
pub use translate::{
    proofread_cues, translate_and_export_track, translate_cues, ProgressUpdate, ReportedName,
    TranslatedTrack, TranslationContext, TranslationGlossaryEntry, TranslationResult,
};
