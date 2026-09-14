//! Agent-backed subtitle tasks (library crate).
//!
//! Translation runs as short-lived, data-isolated tasks through
//! [`AgentInvoker`]; Chat history and general MCP tools are never involved.

pub mod translate;

pub use lumina_core::{AgentInvoker, AgentTaskError, IsolatedAgentTask};
pub use translate::{
    proofread_cues, translate_and_export_track, translate_cues, ProgressUpdate, ReportedName,
    TranslatedTrack, TranslationContext, TranslationGlossaryEntry, TranslationResult,
};
