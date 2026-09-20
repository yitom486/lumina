//! Agent-backed subtitle tasks (library crate).
//!
//! Translation runs as short-lived, data-isolated tasks through
//! [`AgentInvoker`]; Chat history and general MCP tools are never involved.

pub mod chapter;
pub mod prompts;
pub mod translate;

pub use chapter::{
    evidence::{
        build_screenshot_reference, build_transcript_windows, EvidenceBuildError,
        ScreenshotMetadata,
    },
    validate_chapter_output, Chapter, ChapterAgentOutput, ChapterOutput, ChapterValidationContext,
    ChapterValidationReport, EvidenceReference,
};
pub use lumina_core::{AgentInvoker, AgentTaskError, IsolatedAgentTask};
pub use translate::{
    proofread_cues, translate_and_export_track, translate_cues, ProgressUpdate, ReportedName,
    TranslatedTrack, TranslationContext, TranslationGlossaryEntry, TranslationResult,
};
