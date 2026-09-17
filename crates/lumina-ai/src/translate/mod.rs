//! Optional Agent-backed subtitle translation (isolated ACP, no chat pollution).

use lumina_subtitle::error::SubtitleError;
use lumina_subtitle::model::{Cue, Transcript};
use lumina_subtitle::write::normalize_lang_token;

use lumina_core::BatchCheckpoint;

mod agent;
mod batch;
mod context;
mod orchestrator;

#[cfg(test)]
mod tests;

pub use context::{
    canonicalize_person_name, person_name_variants, ReportedName, TranslationContext,
    TranslationGlossaryEntry,
};
pub use orchestrator::{proofread_cues, translate_and_export_track, translate_cues};

/// Cues per agent call. Deliberately modest: a malformed batch fails only
/// its own cues (cheap retry), and long JSON lists garble more often.
/// Raise only with failure-rate evidence, never for call-count savings
/// (total tokens dominate wall time, not call count). Public: checkpoint
/// owners key stored batches by it, so a size change must invalidate them.
pub const TRANSLATE_BATCH_SIZE: usize = 40;
/// Canonical product token for Simplified Chinese. `zh` remains accepted as
/// an input alias so existing callers do not break, but prompts and output
/// naming use the explicit locale.
pub const SIMPLIFIED_CHINESE_TOKEN: &str = "zh-CN";

/// Normalize a translation target without changing the filename-safe token
/// rules owned by `lumina-subtitle`. In particular, `zh` and `zh_CN` are
/// aliases for the product's Simplified Chinese locale.
pub fn normalize_translation_language(target_lang: &str) -> Result<String, SubtitleError> {
    let token = normalize_lang_token(target_lang)?;
    match token.as_str() {
        "zh" | "zh-cn" | "zh_cn" => Ok(SIMPLIFIED_CHINESE_TOKEN.to_string()),
        _ => Ok(token),
    }
}

/// Structured progress for one finished batch. The app forwards it to the
/// UI progress bar; batch text stays human-readable on its own.
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub message: String,
    pub done: Option<usize>,
    pub total: Option<usize>,
}

/// Builds the checkpoint store for one job after the source loads
/// (source, target token, model key). Boxed: owners keep their layout.
pub type CheckpointFactory<'a> = &'a dyn Fn(&Transcript, &str, &str) -> Box<dyn BatchCheckpoint>;

/// Translation output plus backfill candidates. Callers that cannot reach
/// the library simply ignore `reported_names`.
#[derive(Debug, Clone)]
pub struct TranslationResult {
    pub cues: Vec<Cue>,
    pub reported_names: Vec<ReportedName>,
}

/// Exported-track output plus backfill candidates.
#[derive(Debug, Clone)]
pub struct TranslatedTrack {
    pub transcript: Transcript,
    pub reported_names: Vec<ReportedName>,
}
