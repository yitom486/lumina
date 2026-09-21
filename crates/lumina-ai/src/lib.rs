//! Agent-backed subtitle tasks (library crate).
//!
//! Translation runs as short-lived, data-isolated tasks through
//! [`AgentInvoker`]; Chat history and general MCP tools are never involved.

pub mod chapter;
pub mod prompts;
pub mod shortcut;
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
pub use shortcut::{
    validate_chapter_outlook_output, validate_chapter_recap_output, validate_plot_summary_output,
    validate_question_candidates_output, validate_task_output, BulletDetail, BulletItem,
    ChapterOutlookOutput, ChapterRecapOutput, EvidenceDetail, EvidenceItem, PlotSummaryOutput,
    QuestionCandidatesOutput, QuestionDetail, QuestionItem, ShortcutChapter, ShortcutScope,
    ShortcutSpoilerBoundary, CHAPTER_OUTLOOK_CONTRACT_VERSION, CHAPTER_RECAP_CONTRACT_VERSION,
    MAX_SHORTCUT_ITEMS, MAX_SHORTCUT_TEXT_LEN, PLOT_SUMMARY_CONTRACT_VERSION,
    QUESTION_CANDIDATES_CONTRACT_VERSION,
};
pub use translate::{
    proofread_cues, translate_and_export_track, translate_cues, ProgressUpdate, ReportedName,
    TranslatedTrack, TranslationContext, TranslationGlossaryEntry, TranslationResult,
};
