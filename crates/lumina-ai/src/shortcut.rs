//! Typed output contracts for the four companion shortcut tasks.
//!
//! The prompt repository in [`crate::prompts`] owns the version strings for
//! `chapter_recap`, `chapter_outlook`, `plot_summary` and `question_candidates`.
//! This module owns the serde shapes those agents must return and the pure,
//! side-effect-free validators the host can run before accepting an answer.
//!
//! The shapes deliberately mirror the frontend whitelist in
//! `apps/desktop/src/features/acp/shortcutOutput.ts`:
//! `version` (accepted as `contract`), canonical `summary` keys per task,
//! `evidence` objects with `text`/`ref` aliases, `scope`/`chapter` display
//! objects, `spoiler_boundary` display hints, `items`/`points` bullets,
//! `questions`/`candidates` and recap-only `uncertainty`.
//!
//! Unknown JSON fields are ignored on deserialization so the agent can evolve
//! optional presentation content without breaking structural validation.

use serde::{Deserialize, Serialize};

use crate::prompts::{TaskId, ValidationIssue, ValidationReport};

/// Output contract version for the chapter-recap shortcut.
///
/// This is a direct reference to the prompt repository so the two sides can
/// never drift into a second source of truth.
pub const CHAPTER_RECAP_CONTRACT_VERSION: &str =
    TaskId::ChapterRecap.definition().output_contract_version;
/// Output contract version for the chapter-outlook shortcut.
pub const CHAPTER_OUTLOOK_CONTRACT_VERSION: &str =
    TaskId::ChapterOutlook.definition().output_contract_version;
/// Output contract version for the plot-summary shortcut.
pub const PLOT_SUMMARY_CONTRACT_VERSION: &str =
    TaskId::PlotSummary.definition().output_contract_version;
/// Output contract version for the question-candidates shortcut.
pub const QUESTION_CANDIDATES_CONTRACT_VERSION: &str = TaskId::QuestionCandidates
    .definition()
    .output_contract_version;

/// Maximum number of list items accepted for one shortcut output.
///
/// Mirrors the frontend `MAX_ITEMS` truncation limit.
pub const MAX_SHORTCUT_ITEMS: usize = 8;
/// Maximum number of characters accepted for one shortcut text field.
///
/// Mirrors the frontend `readText` truncation limit.
pub const MAX_SHORTCUT_TEXT_LEN: usize = 20_000;

/// One evidence citation understood by the shortcut renderer.
///
/// The frontend accepts either a plain string or an object. The object form
/// reads its display text from `text` (with `quote`/`label`/`title`/
/// `description`/`summary`/`fact` aliases) and its reference from `ref`
/// (with `reference`/`timestamp`/`time_range` aliases).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EvidenceItem {
    /// Plain-text evidence line.
    Text(String),
    /// Structured evidence with text and reference parts.
    Detailed(EvidenceDetail),
}

/// Structured evidence citation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceDetail {
    /// Display text for this citation.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "quote",
        alias = "label",
        alias = "title",
        alias = "description",
        alias = "summary",
        alias = "fact"
    )]
    pub text: Option<String>,
    /// Evidence reference such as a transcript window or time range.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "ref",
        alias = "reference",
        alias = "timestamp",
        alias = "time_range"
    )]
    pub reference: Option<String>,
    /// Optional evidence kind hint (`transcript`/`screenshot`/`chapter`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// One generic bullet used for outlook items and recap uncertainty.
///
/// Accepts either a plain string or an object with `text` (plus `title`/
/// `summary`/`description` aliases), matching the frontend text-array reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BulletItem {
    /// Plain-text bullet.
    Text(String),
    /// Structured bullet with a text field.
    Detailed(BulletDetail),
}

/// Structured generic bullet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulletDetail {
    /// Bullet display text.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "title",
        alias = "summary",
        alias = "description"
    )]
    pub text: Option<String>,
}

/// One question candidate.
///
/// Accepts either a plain string or an object with `question` (plus `prompt`/
/// `text`/`title` aliases). The optional `rationale` is kept for the task
/// contract even though the current frontend question renderer ignores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QuestionItem {
    /// Plain-text question.
    Text(String),
    /// Structured question with an optional rationale.
    Detailed(QuestionDetail),
}

/// Structured question candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionDetail {
    /// The candidate question.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "prompt",
        alias = "text",
        alias = "title"
    )]
    pub question: Option<String>,
    /// Short rationale or evidence hint, when useful.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "reason")]
    pub rationale: Option<String>,
}

/// Display scope for a shortcut card.
///
/// Reads `label` (with `title`/`chapter_title`/`name` aliases), matching the
/// frontend scope reader.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutScope {
    /// Human-readable scope label.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "title",
        alias = "chapter_title",
        alias = "name"
    )]
    pub label: Option<String>,
}

/// Chapter identity shown above a shortcut card.
///
/// Reads `title` (alias `name`) and `position` (alias `timestamp`), matching
/// the frontend chapter-scope reader.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutChapter {
    /// Chapter title, when known.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "name")]
    pub title: Option<String>,
    /// Chapter position hint, when known.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "timestamp")]
    pub position: Option<String>,
}

/// Spoiler hint shown above a shortcut card.
///
/// This is a display hint only. It intentionally does not reuse
/// [`crate::prompts::SpoilerBoundary`]: the frontend renders exactly the
/// three values below, while the prompt input boundary also has `full_media`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutSpoilerBoundary {
    /// Content is bounded by the current playback position.
    CurrentPosition,
    /// Content is bounded by the current chapter.
    CurrentChapter,
    /// No future material is included.
    #[serde(rename = "none")]
    NoSpoiler,
}

/// Structured output for the chapter-recap shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterRecapOutput {
    /// Output contract version, accepted as `contract` on input.
    #[serde(alias = "contract")]
    pub version: String,
    /// Grounded recap prose.
    #[serde(alias = "recap", alias = "content", alias = "text")]
    pub summary: String,
    /// Supporting evidence citations.
    #[serde(default)]
    pub evidence: Vec<EvidenceItem>,
    /// Optional display scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ShortcutScope>,
    /// Optional chapter identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<ShortcutChapter>,
    /// Optional spoiler hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoiler_boundary: Option<ShortcutSpoilerBoundary>,
    /// Explicit uncertainty lines.
    #[serde(default)]
    pub uncertainty: Vec<BulletItem>,
}

/// Structured output for the chapter-outlook shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterOutlookOutput {
    /// Output contract version, accepted as `contract` on input.
    #[serde(alias = "contract")]
    pub version: String,
    /// Short outlook summary prose.
    #[serde(alias = "outlook", alias = "content", alias = "text")]
    pub summary: String,
    /// Distinct outlook bullets. `points` is accepted as an alias.
    ///
    /// An `outlook` array key from older prompts is intentionally not an
    /// alias here: `outlook` as a string already means the summary above, so
    /// a single key cannot deserialize as both shapes. Agents must use
    /// `items` (or `points`) for bullets.
    #[serde(default, alias = "points")]
    pub items: Vec<BulletItem>,
    /// Supporting evidence citations.
    #[serde(default)]
    pub evidence: Vec<EvidenceItem>,
    /// Optional display scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ShortcutScope>,
    /// Optional chapter identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<ShortcutChapter>,
    /// Optional spoiler hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoiler_boundary: Option<ShortcutSpoilerBoundary>,
}

/// Structured output for the plot-summary shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlotSummaryOutput {
    /// Output contract version, accepted as `contract` on input.
    #[serde(alias = "contract")]
    pub version: String,
    /// Coherent spoiler-bounded summary prose.
    #[serde(alias = "content", alias = "text")]
    pub summary: String,
    /// Supporting evidence citations.
    #[serde(default)]
    pub evidence: Vec<EvidenceItem>,
    /// Optional display scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ShortcutScope>,
    /// Optional chapter identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<ShortcutChapter>,
    /// Optional spoiler hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoiler_boundary: Option<ShortcutSpoilerBoundary>,
}

/// Structured output for the question-candidates shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionCandidatesOutput {
    /// Output contract version, accepted as `contract` on input.
    #[serde(alias = "contract")]
    pub version: String,
    /// Distinct answerable questions. `candidates` is accepted as an alias.
    #[serde(default, alias = "candidates")]
    pub questions: Vec<QuestionItem>,
    /// Optional display scope (currently ignored by the question renderer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ShortcutScope>,
    /// Optional chapter identity (currently ignored by the question renderer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<ShortcutChapter>,
    /// Optional spoiler hint (currently ignored by the question renderer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoiler_boundary: Option<ShortcutSpoilerBoundary>,
}

/// Validates a chapter-recap output without I/O or external state.
pub fn validate_chapter_recap_output(output: &ChapterRecapOutput) -> ValidationReport {
    let mut issues = Vec::new();
    check_version(&output.version, CHAPTER_RECAP_CONTRACT_VERSION, &mut issues);
    check_summary(&output.summary, &mut issues);
    validate_evidence(&output.evidence, &mut issues);
    validate_bullets("uncertainty", &output.uncertainty, false, &mut issues);
    validate_scope(output.scope.as_ref(), &mut issues);
    validate_chapter_scope(output.chapter.as_ref(), &mut issues);
    ValidationReport::new(issues)
}

/// Validates a chapter-outlook output without I/O or external state.
pub fn validate_chapter_outlook_output(output: &ChapterOutlookOutput) -> ValidationReport {
    let mut issues = Vec::new();
    check_version(
        &output.version,
        CHAPTER_OUTLOOK_CONTRACT_VERSION,
        &mut issues,
    );
    check_summary(&output.summary, &mut issues);
    if output.items.is_empty() {
        issues.push(issue(
            "missing_items",
            "items",
            "The outlook does not contain any observation items.",
            "a non-empty items array with at most 8 entries",
            "Add at least one grounded outlook item to items (points is accepted on input).",
        ));
    } else {
        validate_bullets("items", &output.items, true, &mut issues);
    }
    validate_evidence(&output.evidence, &mut issues);
    validate_scope(output.scope.as_ref(), &mut issues);
    validate_chapter_scope(output.chapter.as_ref(), &mut issues);
    ValidationReport::new(issues)
}

/// Validates a plot-summary output without I/O or external state.
pub fn validate_plot_summary_output(output: &PlotSummaryOutput) -> ValidationReport {
    let mut issues = Vec::new();
    check_version(&output.version, PLOT_SUMMARY_CONTRACT_VERSION, &mut issues);
    check_summary(&output.summary, &mut issues);
    validate_evidence(&output.evidence, &mut issues);
    validate_scope(output.scope.as_ref(), &mut issues);
    validate_chapter_scope(output.chapter.as_ref(), &mut issues);
    ValidationReport::new(issues)
}

/// Validates a question-candidates output without I/O or external state.
pub fn validate_question_candidates_output(output: &QuestionCandidatesOutput) -> ValidationReport {
    let mut issues = Vec::new();
    check_version(
        &output.version,
        QUESTION_CANDIDATES_CONTRACT_VERSION,
        &mut issues,
    );
    validate_questions(&output.questions, &mut issues);
    validate_scope(output.scope.as_ref(), &mut issues);
    validate_chapter_scope(output.chapter.as_ref(), &mut issues);
    ValidationReport::new(issues)
}

/// Dispatches raw agent text to the task's JSON validator.
///
/// The payload is stripped of a single Markdown ```json fence before
/// deserialization. Returns `Some(report)` for tasks with a JSON output
/// contract (`chapter_recap`, `chapter_outlook`, `plot_summary`,
/// `question_candidates`): an empty report means the output passed, a
/// non-empty report carries either structural issues or a single
/// `invalid_json` issue at `$` when the text is not valid contract JSON.
/// Returns `None` for tasks without a JSON validator (`chapter_segment`,
/// `rewrite_content`), meaning the caller skips validation and passes the
/// reply through.
pub fn validate_task_output(task_id: TaskId, text: &str) -> Option<ValidationReport> {
    let payload = strip_json_fence(text);
    match task_id {
        TaskId::ChapterRecap => match serde_json::from_str::<ChapterRecapOutput>(payload) {
            Ok(output) => Some(validate_chapter_recap_output(&output)),
            Err(_) => Some(invalid_json_report(CHAPTER_RECAP_CONTRACT_VERSION)),
        },
        TaskId::ChapterOutlook => match serde_json::from_str::<ChapterOutlookOutput>(payload) {
            Ok(output) => Some(validate_chapter_outlook_output(&output)),
            Err(_) => Some(invalid_json_report(CHAPTER_OUTLOOK_CONTRACT_VERSION)),
        },
        TaskId::PlotSummary => match serde_json::from_str::<PlotSummaryOutput>(payload) {
            Ok(output) => Some(validate_plot_summary_output(&output)),
            Err(_) => Some(invalid_json_report(PLOT_SUMMARY_CONTRACT_VERSION)),
        },
        TaskId::QuestionCandidates => {
            match serde_json::from_str::<QuestionCandidatesOutput>(payload) {
                Ok(output) => Some(validate_question_candidates_output(&output)),
                Err(_) => Some(invalid_json_report(QUESTION_CANDIDATES_CONTRACT_VERSION)),
            }
        }
        TaskId::ChapterSegment | TaskId::RewriteContent => None,
    }
}

fn invalid_json_report(contract_version: &str) -> ValidationReport {
    ValidationReport::single(ValidationIssue::new(
        "invalid_json",
        "$",
        "The output is not valid JSON for the task contract.",
        format!("valid JSON matching the {contract_version} output contract"),
        format!(
            "Return only valid JSON matching the {contract_version} output contract, without prose or code fences."
        ),
    ))
}

fn strip_json_fence(text: &str) -> &str {
    let trimmed = text.trim();
    let after_open = match trimmed.strip_prefix("```") {
        Some(rest) => rest,
        None => return trimmed,
    };
    let content_with_maybe_tag = match after_open.find('\n') {
        Some(newline) => after_open.get(newline + 1..).unwrap_or_default(),
        None => {
            let inline = after_open.trim_start();
            match inline.get(..4) {
                Some(head) if head.eq_ignore_ascii_case("json") => {
                    inline.get(4..).map(str::trim_start).unwrap_or_default()
                }
                _ => inline,
            }
        }
    };
    let without_close = match content_with_maybe_tag.rfind("```") {
        Some(closing) => content_with_maybe_tag
            .get(..closing)
            .unwrap_or(content_with_maybe_tag),
        None => content_with_maybe_tag,
    };
    without_close.trim()
}

fn check_version(actual: &str, expected: &str, issues: &mut Vec<ValidationIssue>) {
    if actual != expected {
        issues.push(issue(
            "contract_version_mismatch",
            "version",
            "The output contract version does not match the prompt repository.",
            expected,
            "Return the current output contract version without inventing a new one.",
        ));
    }
}

fn check_summary(summary: &str, issues: &mut Vec<ValidationIssue>) {
    if summary.trim().is_empty() {
        issues.push(issue(
            "empty_summary",
            "summary",
            "The summary is blank.",
            "non-empty evidence-grounded prose",
            "Add a concise summary grounded in the supplied evidence.",
        ));
    } else if summary.chars().count() > MAX_SHORTCUT_TEXT_LEN {
        issues.push(issue(
            "summary_too_long",
            "summary",
            "The summary exceeds the supported display length.",
            "at most 20000 characters",
            "Shorten the summary while keeping the grounded facts.",
        ));
    }
}

fn validate_evidence(items: &[EvidenceItem], issues: &mut Vec<ValidationIssue>) {
    if items.len() > MAX_SHORTCUT_ITEMS {
        issues.push(issue(
            "too_many_items",
            "evidence",
            "The evidence list exceeds the supported length.",
            "at most 8 evidence entries",
            "Keep only the strongest evidence entries.",
        ));
    }
    for (index, item) in items.iter().enumerate() {
        let path = format!("evidence[{index}]");
        match item {
            EvidenceItem::Text(text) => {
                if text.trim().is_empty() {
                    issues.push(issue(
                        "empty_evidence_item",
                        path,
                        "An evidence entry is blank.",
                        "non-empty text or a valid reference",
                        "Remove the blank entry or replace it with grounded evidence.",
                    ));
                } else if text.chars().count() > MAX_SHORTCUT_TEXT_LEN {
                    issues.push(issue(
                        "evidence_too_long",
                        path,
                        "An evidence entry exceeds the supported display length.",
                        "at most 20000 characters",
                        "Shorten the evidence entry.",
                    ));
                }
            }
            EvidenceItem::Detailed(detail) => {
                let text = non_empty_trimmed(detail.text.as_deref());
                let reference = non_empty_trimmed(detail.reference.as_deref());
                let kind = non_empty_trimmed(detail.kind.as_deref());
                if text.is_none() && reference.is_none() && !is_known_evidence_kind(kind) {
                    issues.push(issue(
                        "empty_evidence_item",
                        path,
                        "An evidence entry has no usable text, reference, or kind.",
                        "text and/or ref (reference/timestamp/time_range) or a known kind",
                        "Add the cited text or reference, or remove the entry.",
                    ));
                    continue;
                }
                if text.is_some_and(|value| value.chars().count() > MAX_SHORTCUT_TEXT_LEN)
                    || reference.is_some_and(|value| value.chars().count() > MAX_SHORTCUT_TEXT_LEN)
                {
                    issues.push(issue(
                        "evidence_too_long",
                        path,
                        "An evidence entry exceeds the supported display length.",
                        "at most 20000 characters",
                        "Shorten the evidence entry.",
                    ));
                }
            }
        }
    }
}

fn validate_bullets(
    field: &str,
    items: &[BulletItem],
    non_empty_required: bool,
    issues: &mut Vec<ValidationIssue>,
) {
    if items.is_empty() {
        if non_empty_required {
            issues.push(issue(
                "missing_items",
                field,
                "The list does not contain any entries.",
                "a non-empty array with at most 8 entries",
                "Add at least one grounded entry.",
            ));
        }
        return;
    }
    if items.len() > MAX_SHORTCUT_ITEMS {
        issues.push(issue(
            "too_many_items",
            field,
            "The list exceeds the supported length.",
            "at most 8 entries",
            "Keep only the strongest entries.",
        ));
    }
    for (index, item) in items.iter().enumerate() {
        let path = format!("{field}[{index}]");
        let text = match item {
            BulletItem::Text(value) => Some(value.as_str()),
            BulletItem::Detailed(detail) => detail.text.as_deref(),
        };
        let normalized = non_empty_trimmed(text);
        if normalized.is_none() {
            issues.push(issue(
                "empty_bullet_item",
                path,
                "A list entry is blank.",
                "non-empty text",
                "Remove the blank entry or replace it with grounded content.",
            ));
        } else if normalized.is_some_and(|value| value.chars().count() > MAX_SHORTCUT_TEXT_LEN) {
            issues.push(issue(
                "bullet_too_long",
                path,
                "A list entry exceeds the supported display length.",
                "at most 20000 characters",
                "Shorten the entry.",
            ));
        }
    }
}

fn validate_questions(items: &[QuestionItem], issues: &mut Vec<ValidationIssue>) {
    if items.is_empty() {
        issues.push(issue(
            "missing_questions",
            "questions",
            "The output does not contain any question candidates.",
            "a non-empty questions array with at most 8 entries",
            "Add at least one grounded question to questions (candidates is accepted on input).",
        ));
        return;
    }
    if items.len() > MAX_SHORTCUT_ITEMS {
        issues.push(issue(
            "too_many_questions",
            "questions",
            "The questions list exceeds the supported length.",
            "at most 8 entries",
            "Keep only the strongest question candidates.",
        ));
    }
    for (index, item) in items.iter().enumerate() {
        let base = format!("questions[{index}]");
        let (question, rationale) = match item {
            QuestionItem::Text(value) => (Some(value.as_str()), None),
            QuestionItem::Detailed(detail) => {
                (detail.question.as_deref(), detail.rationale.as_deref())
            }
        };
        let normalized = non_empty_trimmed(question);
        if normalized.is_none() {
            issues.push(issue(
                "empty_question",
                format!("{base}.question"),
                "A question candidate is blank.",
                "a non-empty grounded question",
                "Add the question text or remove the blank candidate.",
            ));
        } else if normalized.is_some_and(|value| value.chars().count() > MAX_SHORTCUT_TEXT_LEN) {
            issues.push(issue(
                "question_too_long",
                format!("{base}.question"),
                "A question candidate exceeds the supported display length.",
                "at most 20000 characters",
                "Shorten the question.",
            ));
        }
        if non_empty_trimmed(rationale)
            .is_some_and(|value| value.chars().count() > MAX_SHORTCUT_TEXT_LEN)
        {
            issues.push(issue(
                "rationale_too_long",
                format!("{base}.rationale"),
                "A question rationale exceeds the supported display length.",
                "at most 20000 characters",
                "Shorten the rationale.",
            ));
        }
    }
}

fn validate_scope(scope: Option<&ShortcutScope>, issues: &mut Vec<ValidationIssue>) {
    let Some(scope) = scope else {
        return;
    };
    let label = non_empty_trimmed(scope.label.as_deref());
    if label.is_none() {
        issues.push(issue(
            "empty_scope",
            "scope.label",
            "The display scope is present but blank.",
            "omitted scope or a non-empty label",
            "Omit the scope or provide a concise label.",
        ));
    } else if label.is_some_and(|value| value.chars().count() > MAX_SHORTCUT_TEXT_LEN) {
        issues.push(issue(
            "scope_too_long",
            "scope.label",
            "The display scope exceeds the supported display length.",
            "at most 20000 characters",
            "Shorten the scope label.",
        ));
    }
}

fn validate_chapter_scope(chapter: Option<&ShortcutChapter>, issues: &mut Vec<ValidationIssue>) {
    let Some(chapter) = chapter else {
        return;
    };
    let title = non_empty_trimmed(chapter.title.as_deref());
    let position = non_empty_trimmed(chapter.position.as_deref());
    if title.is_none() && position.is_none() {
        issues.push(issue(
            "empty_chapter_scope",
            "chapter",
            "The chapter scope is present but blank.",
            "omitted chapter or a non-empty title/position",
            "Omit the chapter scope or provide its title or position.",
        ));
    } else if title.is_some_and(|value| value.chars().count() > MAX_SHORTCUT_TEXT_LEN)
        || position.is_some_and(|value| value.chars().count() > MAX_SHORTCUT_TEXT_LEN)
    {
        issues.push(issue(
            "chapter_scope_too_long",
            "chapter",
            "The chapter scope exceeds the supported display length.",
            "at most 20000 characters",
            "Shorten the chapter title or position.",
        ));
    }
}

fn non_empty_trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

fn is_known_evidence_kind(kind: Option<&str>) -> bool {
    matches!(
        kind,
        Some("transcript") | Some("screenshot") | Some("chapter")
    )
}

fn issue(
    error_code: impl Into<String>,
    field_path: impl Into<String>,
    reason: impl Into<String>,
    expected: impl Into<String>,
    repair_suggestion: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue::new(error_code, field_path, reason, expected, repair_suggestion)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_recap() -> ChapterRecapOutput {
        ChapterRecapOutput {
            version: CHAPTER_RECAP_CONTRACT_VERSION.to_string(),
            summary: "The group restates the current goal before moving on.".to_string(),
            evidence: vec![EvidenceItem::Detailed(EvidenceDetail {
                text: Some("A line of dialogue.".to_string()),
                reference: Some("window-1".to_string()),
                kind: None,
            })],
            scope: Some(ShortcutScope {
                label: Some("Current chapter".to_string()),
            }),
            chapter: Some(ShortcutChapter {
                title: Some("The first turn".to_string()),
                position: None,
            }),
            spoiler_boundary: Some(ShortcutSpoilerBoundary::CurrentChapter),
            uncertainty: vec![BulletItem::Text("Whether the plan will hold.".to_string())],
        }
    }

    fn valid_outlook() -> ChapterOutlookOutput {
        ChapterOutlookOutput {
            version: CHAPTER_OUTLOOK_CONTRACT_VERSION.to_string(),
            summary: "Watch how responsibility shifts in the next scene.".to_string(),
            items: vec![
                BulletItem::Text("Notice who speaks first.".to_string()),
                BulletItem::Detailed(BulletDetail {
                    text: Some("Track the background music.".to_string()),
                }),
            ],
            evidence: vec![EvidenceItem::Text("A representative line.".to_string())],
            scope: None,
            chapter: None,
            spoiler_boundary: Some(ShortcutSpoilerBoundary::CurrentPosition),
        }
    }

    fn valid_summary() -> PlotSummaryOutput {
        PlotSummaryOutput {
            version: PLOT_SUMMARY_CONTRACT_VERSION.to_string(),
            summary: "The available plot follows the group as they regroup.".to_string(),
            evidence: vec![EvidenceItem::Detailed(EvidenceDetail {
                text: Some("Regroup dialogue.".to_string()),
                reference: None,
                kind: Some("transcript".to_string()),
            })],
            scope: None,
            chapter: None,
            spoiler_boundary: None,
        }
    }

    fn valid_questions() -> QuestionCandidatesOutput {
        QuestionCandidatesOutput {
            version: QUESTION_CANDIDATES_CONTRACT_VERSION.to_string(),
            questions: vec![
                QuestionItem::Text("Why did the group pause here?".to_string()),
                QuestionItem::Detailed(QuestionDetail {
                    question: Some("What does the pause reveal?".to_string()),
                    rationale: Some("It follows the turning line.".to_string()),
                }),
            ],
            scope: None,
            chapter: None,
            spoiler_boundary: None,
        }
    }

    fn has_code(report: &ValidationReport, code: &str) -> bool {
        report.issues.iter().any(|item| item.error_code == code)
    }

    #[test]
    fn recap_contract_versions_match_prompt_repository() {
        assert_eq!(
            CHAPTER_RECAP_CONTRACT_VERSION,
            TaskId::ChapterRecap.definition().output_contract_version
        );
        assert_eq!(
            CHAPTER_OUTLOOK_CONTRACT_VERSION,
            TaskId::ChapterOutlook.definition().output_contract_version
        );
        assert_eq!(
            PLOT_SUMMARY_CONTRACT_VERSION,
            TaskId::PlotSummary.definition().output_contract_version
        );
        assert_eq!(
            QUESTION_CANDIDATES_CONTRACT_VERSION,
            TaskId::QuestionCandidates
                .definition()
                .output_contract_version
        );
    }

    #[test]
    fn valid_recap_passes() {
        let report = validate_chapter_recap_output(&valid_recap());
        assert!(report.is_empty());
    }

    #[test]
    fn recap_rejects_blank_summary() {
        let mut output = valid_recap();
        output.summary = "   ".to_string();
        let report = validate_chapter_recap_output(&output);
        assert!(has_code(&report, "empty_summary"));
    }

    #[test]
    fn recap_rejects_wrong_contract_version() {
        let mut output = valid_recap();
        output.version = "chapter_recap.v999".to_string();
        let report = validate_chapter_recap_output(&output);
        assert!(has_code(&report, "contract_version_mismatch"));
    }

    #[test]
    fn recap_accepts_frontend_alias_keys() {
        let json = format!(
            r#"{{"contract": "{version}", "recap": "Alias recap.", "evidence": [{{"quote": "Quoted line.", "reference": "window-1"}}], "scope": {{"title": "Scope title"}}, "chapter": {{"name": "Chapter one", "timestamp": "00:01"}}, "spoiler_boundary": "current_chapter", "uncertainty": ["Check this."]}}"#,
            version = CHAPTER_RECAP_CONTRACT_VERSION
        );
        let parsed = match serde_json::from_str::<ChapterRecapOutput>(&json) {
            Ok(output) => output,
            Err(error) => panic!("recap aliases should deserialize: {error}"),
        };
        assert!(validate_chapter_recap_output(&parsed).is_empty());
    }

    #[test]
    fn valid_outlook_passes() {
        let report = validate_chapter_outlook_output(&valid_outlook());
        assert!(report.is_empty());
    }

    #[test]
    fn outlook_rejects_missing_items() {
        let mut output = valid_outlook();
        output.items.clear();
        let report = validate_chapter_outlook_output(&output);
        assert!(has_code(&report, "missing_items"));
    }

    #[test]
    fn outlook_rejects_wrong_contract_version() {
        let mut output = valid_outlook();
        output.version = "chapter_outlook.v999".to_string();
        let report = validate_chapter_outlook_output(&output);
        assert!(has_code(&report, "contract_version_mismatch"));
    }

    #[test]
    fn outlook_accepts_points_alias_and_rejects_too_many() {
        let json = format!(
            r#"{{"version": "{version}", "summary": "Alias outlook.", "points": ["one", "two"]}}"#,
            version = CHAPTER_OUTLOOK_CONTRACT_VERSION
        );
        let parsed = match serde_json::from_str::<ChapterOutlookOutput>(&json) {
            Ok(output) => output,
            Err(error) => panic!("outlook points alias should deserialize: {error}"),
        };
        assert!(validate_chapter_outlook_output(&parsed).is_empty());

        let mut crowded = valid_outlook();
        crowded.items = (0..9)
            .map(|index| BulletItem::Text(format!("item {index}")))
            .collect();
        assert!(has_code(
            &validate_chapter_outlook_output(&crowded),
            "too_many_items"
        ));
    }

    #[test]
    fn valid_plot_summary_passes() {
        let report = validate_plot_summary_output(&valid_summary());
        assert!(report.is_empty());
    }

    #[test]
    fn plot_summary_rejects_blank_summary() {
        let mut output = valid_summary();
        output.summary.clear();
        let report = validate_plot_summary_output(&output);
        assert!(has_code(&report, "empty_summary"));
    }

    #[test]
    fn plot_summary_rejects_wrong_contract_version() {
        let mut output = valid_summary();
        output.version = "plot_summary.v999".to_string();
        let report = validate_plot_summary_output(&output);
        assert!(has_code(&report, "contract_version_mismatch"));
    }

    #[test]
    fn plot_summary_missing_summary_field_fails_deserialization() {
        let json = format!(
            r#"{{"version": "{version}", "evidence": []}}"#,
            version = PLOT_SUMMARY_CONTRACT_VERSION
        );
        assert!(serde_json::from_str::<PlotSummaryOutput>(&json).is_err());
    }

    #[test]
    fn valid_questions_pass() {
        let report = validate_question_candidates_output(&valid_questions());
        assert!(report.is_empty());
    }

    #[test]
    fn questions_reject_empty_list() {
        let mut output = valid_questions();
        output.questions.clear();
        let report = validate_question_candidates_output(&output);
        assert!(has_code(&report, "missing_questions"));
    }

    #[test]
    fn questions_reject_wrong_contract_version() {
        let mut output = valid_questions();
        output.version = "question_candidates.v999".to_string();
        let report = validate_question_candidates_output(&output);
        assert!(has_code(&report, "contract_version_mismatch"));
    }

    #[test]
    fn questions_accept_candidates_alias_and_reject_blank() {
        let json = format!(
            r#"{{"version": "{version}", "candidates": ["Why pause?", {{"prompt": "What changes?"}}]}}"#,
            version = QUESTION_CANDIDATES_CONTRACT_VERSION
        );
        let parsed = match serde_json::from_str::<QuestionCandidatesOutput>(&json) {
            Ok(output) => output,
            Err(error) => panic!("candidates alias should deserialize: {error}"),
        };
        assert!(validate_question_candidates_output(&parsed).is_empty());

        let mut blank = valid_questions();
        blank.questions = vec![QuestionItem::Text("   ".to_string())];
        assert!(has_code(
            &validate_question_candidates_output(&blank),
            "empty_question"
        ));
    }

    #[test]
    fn shortcut_reports_are_prompt_compatible() {
        let mut output = valid_recap();
        output.summary = String::new();
        output.version = "bad".to_string();
        let report = validate_chapter_recap_output(&output);
        for item in &report.issues {
            assert!(!item.error_code.trim().is_empty());
            assert!(!item.field_path.trim().is_empty());
            assert!(!item.reason.trim().is_empty());
            assert!(!item.expected.trim().is_empty());
            assert!(!item.repair_suggestion.trim().is_empty());
        }
    }

    fn invalid_json_code(report: Option<ValidationReport>) -> String {
        match report {
            Some(report) => {
                assert_eq!(report.issues.len(), 1);
                let issue = &report.issues[0];
                assert_eq!(issue.field_path, "$");
                issue.error_code.clone()
            }
            None => panic!("expected a validation report for illegal JSON"),
        }
    }

    #[test]
    fn dispatcher_rejects_illegal_json_for_recap() {
        assert_eq!(
            invalid_json_code(validate_task_output(
                TaskId::ChapterRecap,
                "not json at all"
            )),
            "invalid_json"
        );
    }

    #[test]
    fn dispatcher_rejects_illegal_json_for_outlook() {
        assert_eq!(
            invalid_json_code(validate_task_output(
                TaskId::ChapterOutlook,
                "{broken json,,,"
            )),
            "invalid_json"
        );
    }

    #[test]
    fn dispatcher_rejects_illegal_json_for_plot_summary() {
        assert_eq!(
            invalid_json_code(validate_task_output(TaskId::PlotSummary, "")),
            "invalid_json"
        );
    }

    #[test]
    fn dispatcher_rejects_illegal_json_for_question_candidates() {
        assert_eq!(
            invalid_json_code(validate_task_output(
                TaskId::QuestionCandidates,
                "```json\nnot json\n```"
            )),
            "invalid_json"
        );
    }

    #[test]
    fn dispatcher_skips_tasks_without_json_validators() {
        assert!(validate_task_output(TaskId::ChapterSegment, "anything").is_none());
        assert!(validate_task_output(TaskId::RewriteContent, "{\"any\": 1}").is_none());
    }

    #[test]
    fn dispatcher_accepts_fenced_valid_json() {
        let json = format!(
            r#"{{"version": "{version}", "summary": "Grounded recap.", "evidence": []}}"#,
            version = CHAPTER_RECAP_CONTRACT_VERSION
        );
        let fenced = format!("```json\n{json}\n```");
        let report = match validate_task_output(TaskId::ChapterRecap, &fenced) {
            Some(report) => report,
            None => panic!("recap should have a validator"),
        };
        assert!(report.is_empty());
    }

    #[test]
    fn dispatcher_surfaces_structural_issues_not_invalid_json() {
        let json = r#"{"version": "plot_summary.v999", "summary": "Grounded.", "evidence": []}"#
            .to_string();
        let report = match validate_task_output(TaskId::PlotSummary, &json) {
            Some(report) => report,
            None => panic!("plot summary should have a validator"),
        };
        assert!(has_code(&report, "contract_version_mismatch"));
    }
}
