//! Structured chapter-agent output and pure, side-effect-free validation.
//!
//! This module deliberately knows nothing about ACP, Tauri, persistence or
//! UI. The host supplies the media duration, spoiler policy and the evidence
//! registry that the agent was allowed to cite.

pub mod evidence;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::prompts::{
    ScreenshotReference, SpoilerBoundary, TranscriptWindow, ValidationIssue, ValidationReport,
    ViewingContext,
};

/// The version of the chapter-segmentation output contract.
pub const CHAPTER_OUTPUT_CONTRACT_VERSION: &str = "chapter_segment.v1";

/// Structured output returned by a chapter-segmentation agent.
///
/// Unknown JSON fields are intentionally ignored by serde. That lets the
/// agent evolve optional presentation content without making the structural
/// validator reject otherwise usable chapters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterAgentOutput {
    /// Chapters in ascending timeline order.
    #[serde(default)]
    pub chapters: Vec<ChapterOutput>,
    /// Optional prose shown before the chapter list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recap: Option<String>,
    /// Optional spoiler-bounded outlook prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outlook: Option<String>,
    /// Optional points the viewer may watch for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_points: Option<Vec<String>>,
    /// Optional questions the viewer may choose to explore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<String>>,
}

/// One ordered, evidence-grounded chapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterOutput {
    /// Stable identifier chosen for this output revision.
    pub id: String,
    /// Human-readable chapter title.
    pub title: String,
    /// Inclusive chapter start anchor in milliseconds.
    pub start_ms: u64,
    /// Exclusive chapter end anchor in milliseconds.
    pub end_ms: u64,
    /// Natural-language mainline; no fixed sentence or word count is used.
    pub mainline: String,
    /// Subtitle-window and screenshot citations supporting this chapter.
    #[serde(default, alias = "evidence_references", alias = "references")]
    pub evidence: Vec<EvidenceReference>,
}

/// Short alias for callers that refer to a chapter DTO simply as a chapter.
pub type Chapter = ChapterOutput;

/// A citation to one of the evidence resources supplied to the agent.
///
/// The tagged representation keeps subtitle and screenshot references
/// explicit and prevents an ID from being silently interpreted as the wrong
/// resource kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvidenceReference {
    /// A bounded subtitle/transcript window.
    #[serde(alias = "subtitle")]
    Transcript { window_id: String },
    /// A representative screenshot asset.
    Screenshot { asset_id: String },
}

impl EvidenceReference {
    /// Creates a transcript-window citation.
    pub fn transcript(window_id: impl Into<String>) -> Self {
        Self::Transcript {
            window_id: window_id.into(),
        }
    }

    /// Creates a screenshot citation.
    pub fn screenshot(asset_id: impl Into<String>) -> Self {
        Self::Screenshot {
            asset_id: asset_id.into(),
        }
    }

    fn stable_key(&self) -> String {
        match self {
            Self::Transcript { window_id } => format!("transcript:{window_id}"),
            Self::Screenshot { asset_id } => format!("screenshot:{asset_id}"),
        }
    }
}

/// Read-only input required to validate one chapter-agent output.
///
/// `transcript_windows` and `screenshots` are the resources available to the
/// agent for this media. An empty registry means that a citation cannot be
/// proven to exist and is therefore a hard validation error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterValidationContext {
    /// Duration of the current media in milliseconds.
    pub media_duration_ms: u64,
    /// Viewing position and spoiler policy, when the caller has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewing: Option<ViewingContext>,
    /// End of the current chapter, needed for `CurrentChapter` boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_chapter_end_ms: Option<u64>,
    /// Subtitle windows available for citation.
    #[serde(default)]
    pub transcript_windows: Vec<TranscriptWindow>,
    /// Screenshot assets available for citation.
    #[serde(default)]
    pub screenshots: Vec<ScreenshotReference>,
}

impl ChapterValidationContext {
    /// Creates a context for a media with no spoiler restriction and no
    /// evidence registry entries yet.
    pub fn new(media_duration_ms: u64) -> Self {
        Self {
            media_duration_ms,
            ..Self::default()
        }
    }

    /// Adds the viewing position and spoiler boundary.
    pub fn with_viewing(mut self, viewing: ViewingContext) -> Self {
        self.viewing = Some(viewing);
        self
    }

    /// Adds the current chapter's end anchor.
    pub fn with_current_chapter_end_ms(mut self, end_ms: u64) -> Self {
        self.current_chapter_end_ms = Some(end_ms);
        self
    }

    /// Adds one subtitle window to the evidence registry.
    pub fn with_transcript_window(mut self, window: TranscriptWindow) -> Self {
        self.transcript_windows.push(window);
        self
    }

    /// Adds one screenshot asset to the evidence registry.
    pub fn with_screenshot(mut self, screenshot: ScreenshotReference) -> Self {
        self.screenshots.push(screenshot);
        self
    }
}

/// Validation output with hard failures and non-blocking warnings separated.
///
/// Both buckets use the prompt repository's stable `ValidationReport` shape,
/// so the hard-error bucket can be appended as an incremental retry report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterValidationReport {
    /// Structural or safety failures that make the output unusable.
    pub hard_errors: ValidationReport,
    /// Actionable quality observations that do not require a retry.
    pub warnings: ValidationReport,
}

impl ChapterValidationReport {
    /// Returns true when no hard validation error was found.
    pub fn is_valid(&self) -> bool {
        self.hard_errors.is_empty()
    }

    /// Returns the prompt-compatible report for an incremental retry.
    pub fn hard_error_report(&self) -> &ValidationReport {
        &self.hard_errors
    }

    /// Returns the non-blocking prompt-compatible report.
    pub fn warning_report(&self) -> &ValidationReport {
        &self.warnings
    }
}

/// Validates a chapter-agent output without I/O or external state.
pub fn validate_chapter_output(
    output: &ChapterAgentOutput,
    context: &ChapterValidationContext,
) -> ChapterValidationReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if context.media_duration_ms == 0 {
        errors.push(issue(
            "invalid_media_duration",
            "context.media_duration_ms",
            "The media duration must be greater than zero.",
            "a positive duration in milliseconds",
            "Provide the current media duration before validating chapters.",
        ));
    }

    if output.chapters.is_empty() {
        errors.push(issue(
            "missing_chapters",
            "chapters",
            "The output does not contain any chapters.",
            "a non-empty ordered chapters array",
            "Return at least one meaningful chapter with its required fields and evidence.",
        ));
    }

    validate_optional_content(output, &mut warnings);

    let spoiler_limit_ms = spoiler_limit_ms(context);
    let mut chapter_ids = BTreeSet::new();
    let mut previous: Option<&ChapterOutput> = None;

    for (chapter_index, chapter) in output.chapters.iter().enumerate() {
        let chapter_path = format!("chapters[{chapter_index}]");
        let id_path = format!("{chapter_path}.id");
        let title_path = format!("{chapter_path}.title");
        let start_path = format!("{chapter_path}.start_ms");
        let end_path = format!("{chapter_path}.end_ms");

        if chapter.id.trim().is_empty() {
            errors.push(issue(
                "empty_chapter_id",
                id_path,
                "The chapter identifier is blank.",
                "a stable non-empty string",
                "Provide a stable identifier for this chapter.",
            ));
        } else if !chapter_ids.insert(chapter.id.trim().to_string()) {
            errors.push(issue(
                "duplicate_chapter_id",
                id_path,
                "The chapter identifier is reused by another chapter.",
                "a unique identifier within this output",
                "Rename the chapter so every chapter identifier is unique.",
            ));
        }

        if chapter.title.trim().is_empty() {
            errors.push(issue(
                "empty_chapter_title",
                title_path,
                "The chapter title is blank.",
                "a non-empty human-readable title",
                "Add a concise title grounded in the supplied evidence.",
            ));
        }

        if chapter.mainline.trim().is_empty() {
            errors.push(issue(
                "empty_chapter_mainline",
                format!("{chapter_path}.mainline"),
                "The chapter has no mainline.",
                "a non-empty evidence-grounded description",
                "Add the chapter's central movement or subject without imposing a fixed length.",
            ));
        }

        if chapter.start_ms >= chapter.end_ms {
            errors.push(issue(
                "invalid_chapter_interval",
                chapter_path.clone(),
                "The chapter start must be earlier than its end.",
                "start_ms < end_ms",
                "Adjust the anchors to form a positive-length interval.",
            ));
        }

        if chapter.start_ms > context.media_duration_ms {
            errors.push(issue(
                "chapter_out_of_media_bounds",
                start_path,
                "The chapter start is after the media duration.",
                "0 <= start_ms <= media_duration_ms",
                "Move the start anchor inside the media duration.",
            ));
        }
        if chapter.end_ms > context.media_duration_ms {
            errors.push(issue(
                "chapter_out_of_media_bounds",
                end_path,
                "The chapter end is after the media duration.",
                "0 < end_ms <= media_duration_ms",
                "Move the end anchor inside the media duration.",
            ));
        }

        if let Some(limit_ms) = spoiler_limit_ms {
            if chapter.start_ms > limit_ms {
                errors.push(issue(
                    "spoiler_boundary_exceeded",
                    format!("{chapter_path}.start_ms"),
                    "The chapter starts after the allowed viewing boundary.",
                    "start_ms <= the configured spoiler boundary",
                    "Remove future chapters or keep their anchors at or before the allowed boundary.",
                ));
            }
            if chapter.end_ms > limit_ms {
                errors.push(issue(
                    "spoiler_boundary_exceeded",
                    format!("{chapter_path}.end_ms"),
                    "The chapter extends beyond the allowed viewing boundary.",
                    "end_ms <= the configured spoiler boundary",
                    "Trim the chapter to watched material or omit future material.",
                ));
            }
        }

        if let Some(previous_chapter) = previous {
            if chapter.start_ms < previous_chapter.start_ms {
                errors.push(issue(
                    "chapters_not_ordered",
                    format!("{chapter_path}.start_ms"),
                    "Chapter start anchors are not monotonically increasing.",
                    "each chapter start_ms >= the preceding chapter start_ms",
                    "Reorder the chapters by their start anchors.",
                ));
            }
            if chapter.start_ms < previous_chapter.end_ms {
                errors.push(issue(
                    "chapter_overlap",
                    chapter_path.clone(),
                    "This chapter overlaps the preceding chapter.",
                    "start_ms >= the preceding chapter end_ms",
                    "Move the boundary so adjacent chapters do not cover the same interval.",
                ));
            }
        }

        validate_evidence(
            chapter,
            &chapter_path,
            context,
            spoiler_limit_ms,
            &mut errors,
            &mut warnings,
        );

        previous = Some(chapter);
    }

    ChapterValidationReport {
        hard_errors: ValidationReport::new(errors),
        warnings: ValidationReport::new(warnings),
    }
}

/// Concise alias for callers that prefer a conventional validator name.
pub fn validate(
    output: &ChapterAgentOutput,
    context: &ChapterValidationContext,
) -> ChapterValidationReport {
    validate_chapter_output(output, context)
}

fn validate_optional_content(output: &ChapterAgentOutput, warnings: &mut Vec<ValidationIssue>) {
    if output
        .recap
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        warnings.push(issue(
            "empty_optional_content",
            "recap",
            "The optional recap is present but blank.",
            "omitted content or non-empty prose",
            "Omit the recap or provide grounded prose if a recap is useful.",
        ));
    }
    if output
        .outlook
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        warnings.push(issue(
            "empty_optional_content",
            "outlook",
            "The optional outlook is present but blank.",
            "omitted content or non-empty prose",
            "Omit the outlook or provide grounded prose if an outlook is useful.",
        ));
    }
    validate_optional_list("watch_points", output.watch_points.as_deref(), warnings);
    validate_optional_list("questions", output.questions.as_deref(), warnings);
}

fn validate_optional_list(
    field_name: &str,
    values: Option<&[String]>,
    warnings: &mut Vec<ValidationIssue>,
) {
    let Some(values) = values else {
        return;
    };
    for (index, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            warnings.push(issue(
                "empty_optional_content",
                format!("{field_name}[{index}]"),
                "An optional item is present but blank.",
                "omitted item or non-empty prose",
                "Remove the blank item or replace it with grounded content.",
            ));
        }
    }
}

fn validate_evidence(
    chapter: &ChapterOutput,
    chapter_path: &str,
    context: &ChapterValidationContext,
    spoiler_limit_ms: Option<u64>,
    errors: &mut Vec<ValidationIssue>,
    warnings: &mut Vec<ValidationIssue>,
) {
    if chapter.evidence.is_empty() {
        errors.push(issue(
            "missing_evidence",
            format!("{chapter_path}.evidence"),
            "The chapter has no evidence references.",
            "at least one valid transcript or screenshot reference",
            "Cite the supplied subtitle window or representative screenshot that supports the chapter.",
        ));
        return;
    }

    let mut seen_evidence = BTreeSet::new();
    for (evidence_index, reference) in chapter.evidence.iter().enumerate() {
        let evidence_path = format!("{chapter_path}.evidence[{evidence_index}]");
        if !seen_evidence.insert(reference.stable_key()) {
            warnings.push(issue(
                "duplicate_evidence_reference",
                evidence_path.clone(),
                "The same evidence reference is repeated in this chapter.",
                "each evidence reference used at most once",
                "Keep one citation and use another supported reference if it adds evidence.",
            ));
        }

        match reference {
            EvidenceReference::Transcript { window_id } => {
                let reference_path = format!("{evidence_path}.window_id");
                if window_id.trim().is_empty() {
                    errors.push(issue(
                        "invalid_evidence_reference",
                        reference_path,
                        "The transcript window identifier is blank.",
                        "a non-empty transcript window identifier",
                        "Use the identifier of a supplied transcript window.",
                    ));
                    continue;
                }
                let Some(window) = context
                    .transcript_windows
                    .iter()
                    .find(|candidate| candidate.window_id == *window_id)
                else {
                    errors.push(issue(
                        "unknown_transcript_reference",
                        reference_path,
                        "The cited transcript window is not available for this media.",
                        "an identifier from the supplied transcript windows",
                        "Replace it with a transcript window from the current media context.",
                    ));
                    continue;
                };

                if window.start_ms >= window.end_ms {
                    errors.push(issue(
                        "invalid_evidence_reference",
                        evidence_path.clone(),
                        "The supplied transcript window does not form a valid interval.",
                        "transcript window start_ms < end_ms",
                        "Cite a transcript window with a positive-length interval.",
                    ));
                } else if window.end_ms <= chapter.start_ms || window.start_ms >= chapter.end_ms {
                    errors.push(issue(
                        "evidence_outside_chapter",
                        evidence_path.clone(),
                        "The cited transcript window does not intersect this chapter.",
                        "a transcript window overlapping the chapter interval",
                        "Cite dialogue from within this chapter's time range.",
                    ));
                }
                if let Some(limit_ms) = spoiler_limit_ms {
                    if window.end_ms > limit_ms {
                        errors.push(issue(
                            "spoiler_boundary_exceeded",
                            evidence_path,
                            "The cited transcript window includes material beyond the viewing boundary.",
                            "the complete citation ending at or before the spoiler boundary",
                            "Use only a transcript window fully contained in the allowed material.",
                        ));
                    }
                }
            }
            EvidenceReference::Screenshot { asset_id } => {
                let reference_path = format!("{evidence_path}.asset_id");
                if asset_id.trim().is_empty() {
                    errors.push(issue(
                        "invalid_evidence_reference",
                        reference_path,
                        "The screenshot asset identifier is blank.",
                        "a non-empty screenshot asset identifier",
                        "Use the identifier of a supplied screenshot asset.",
                    ));
                    continue;
                }
                let Some(screenshot) = context
                    .screenshots
                    .iter()
                    .find(|candidate| candidate.asset_id == *asset_id)
                else {
                    errors.push(issue(
                        "unknown_screenshot_reference",
                        reference_path,
                        "The cited screenshot is not available for this media.",
                        "an asset identifier from the supplied screenshots",
                        "Replace it with a screenshot asset from the current media context.",
                    ));
                    continue;
                };

                if screenshot.timestamp_ms < chapter.start_ms
                    || screenshot.timestamp_ms >= chapter.end_ms
                {
                    errors.push(issue(
                        "evidence_outside_chapter",
                        evidence_path.clone(),
                        "The cited screenshot is outside this chapter's interval.",
                        "a screenshot timestamp within the chapter interval",
                        "Cite a screenshot captured during this chapter.",
                    ));
                }
                if screenshot.timestamp_ms > context.media_duration_ms {
                    errors.push(issue(
                        "invalid_evidence_reference",
                        evidence_path.clone(),
                        "The cited screenshot timestamp is outside the media duration.",
                        "0 <= timestamp_ms <= media_duration_ms",
                        "Cite a screenshot captured within the current media.",
                    ));
                }
                if let Some(limit_ms) = spoiler_limit_ms {
                    if screenshot.timestamp_ms > limit_ms {
                        errors.push(issue(
                            "spoiler_boundary_exceeded",
                            evidence_path,
                            "The cited screenshot is beyond the viewing boundary.",
                            "a screenshot timestamp at or before the spoiler boundary",
                            "Use only a screenshot from the allowed material.",
                        ));
                    }
                }
            }
        }
    }
}

fn spoiler_limit_ms(context: &ChapterValidationContext) -> Option<u64> {
    let viewing = context.viewing?;
    let limit = match viewing.spoiler_boundary {
        SpoilerBoundary::CurrentPosition => viewing.position_ms,
        SpoilerBoundary::CurrentChapter => context
            .current_chapter_end_ms
            .unwrap_or(viewing.position_ms),
        SpoilerBoundary::FullMedia => return None,
    };
    Some(limit.min(context.media_duration_ms))
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

    fn context() -> ChapterValidationContext {
        ChapterValidationContext::new(60_000)
            .with_transcript_window(TranscriptWindow {
                window_id: "window-1".to_string(),
                start_ms: 1_000,
                end_ms: 9_000,
                lines: Vec::new(),
            })
            .with_transcript_window(TranscriptWindow {
                window_id: "window-2".to_string(),
                start_ms: 10_000,
                end_ms: 19_000,
                lines: Vec::new(),
            })
            .with_screenshot(ScreenshotReference {
                asset_id: "shot-1".to_string(),
                timestamp_ms: 5_000,
                resource_ref: "capture://shot-1".to_string(),
                note: None,
            })
    }

    fn valid_output() -> ChapterAgentOutput {
        ChapterAgentOutput {
            chapters: vec![ChapterOutput {
                id: "chapter-1".to_string(),
                title: "The first turn".to_string(),
                start_ms: 0,
                end_ms: 10_000,
                mainline: "The group identifies a new problem and chooses how to respond."
                    .to_string(),
                evidence: vec![
                    EvidenceReference::transcript("window-1"),
                    EvidenceReference::screenshot("shot-1"),
                ],
            }],
            recap: Some("A natural recap with no fixed sentence budget.".to_string()),
            outlook: None,
            watch_points: Some(vec![
                "Notice how the group divides responsibility.".to_string()
            ]),
            questions: None,
        }
    }

    fn has_code(report: &ValidationReport, code: &str) -> bool {
        report.issues.iter().any(|issue| issue.error_code == code)
    }

    #[test]
    fn valid_output_passes_and_keeps_optional_content_free_form() {
        let output = valid_output();
        let report = validate_chapter_output(&output, &context());

        assert!(report.is_valid());
        assert!(report.hard_errors.is_empty());
        assert!(report.warnings.is_empty());

        let mut concise_output = output;
        concise_output.chapters[0].mainline = "A turn.".to_string();
        assert!(validate_chapter_output(&concise_output, &context()).is_valid());
    }

    #[test]
    fn invalid_media_boundary_and_interval_are_hard_errors() {
        let mut output = valid_output();
        output.chapters[0].start_ms = 70_000;
        output.chapters[0].end_ms = 60_001;

        let report = validate(&output, &context());

        assert!(has_code(&report.hard_errors, "invalid_chapter_interval"));
        assert!(has_code(&report.hard_errors, "chapter_out_of_media_bounds"));
    }

    #[test]
    fn overlapping_chapters_are_rejected() {
        let mut output = valid_output();
        output.chapters.push(ChapterOutput {
            id: "chapter-2".to_string(),
            title: "The consequence".to_string(),
            start_ms: 9_000,
            end_ms: 20_000,
            mainline: "The decision changes the immediate situation.".to_string(),
            evidence: vec![EvidenceReference::transcript("window-2")],
        });

        let report = validate_chapter_output(&output, &context());

        assert!(!report.is_valid());
        assert!(has_code(&report.hard_errors, "chapter_overlap"));
    }

    #[test]
    fn future_material_crossing_spoiler_boundary_is_rejected() {
        let mut output = valid_output();
        output.chapters[0].end_ms = 20_000;
        output.chapters[0].evidence = vec![EvidenceReference::transcript("window-2")];
        let restricted_context = context().with_viewing(ViewingContext {
            position_ms: 12_000,
            spoiler_boundary: SpoilerBoundary::CurrentPosition,
        });

        let report = validate_chapter_output(&output, &restricted_context);

        assert!(has_code(&report.hard_errors, "spoiler_boundary_exceeded"));
    }

    #[test]
    fn duplicate_evidence_and_blank_optional_content_are_warnings() {
        let mut output = valid_output();
        output.chapters[0].evidence = vec![
            EvidenceReference::transcript("window-1"),
            EvidenceReference::transcript("window-1"),
        ];
        output.recap = Some("  ".to_string());

        let report = validate_chapter_output(&output, &context());

        assert!(report.is_valid());
        assert!(has_code(&report.warnings, "duplicate_evidence_reference"));
        assert!(has_code(&report.warnings, "empty_optional_content"));
    }

    #[test]
    fn empty_title_mainline_and_unknown_evidence_are_hard_errors() {
        let mut output = valid_output();
        output.chapters[0].title = "\t".to_string();
        output.chapters[0].mainline = String::new();
        output.chapters[0].evidence = vec![EvidenceReference::Transcript {
            window_id: "missing-window".to_string(),
        }];

        let report = validate_chapter_output(&output, &context());

        assert!(has_code(&report.hard_errors, "empty_chapter_title"));
        assert!(has_code(&report.hard_errors, "empty_chapter_mainline"));
        assert!(has_code(
            &report.hard_errors,
            "unknown_transcript_reference"
        ));
    }

    #[test]
    fn malformed_evidence_ids_are_hard_errors() {
        let mut output = valid_output();
        output.chapters[0].evidence = vec![EvidenceReference::Screenshot {
            asset_id: " ".to_string(),
        }];

        let report = validate_chapter_output(&output, &context());

        assert!(has_code(&report.hard_errors, "invalid_evidence_reference"));
    }

    #[test]
    fn reports_are_prompt_compatible_and_business_facing() {
        let mut output = valid_output();
        output.chapters[0].end_ms = 70_000;
        let report = validate_chapter_output(&output, &context());

        assert_eq!(report.hard_error_report(), &report.hard_errors);
        for issue in report
            .hard_errors
            .issues
            .iter()
            .chain(report.warnings.issues.iter())
        {
            assert!(!issue.error_code.trim().is_empty());
            assert!(!issue.field_path.trim().is_empty());
            assert!(!issue.reason.trim().is_empty());
            assert!(!issue.expected.trim().is_empty());
            assert!(!issue.repair_suggestion.trim().is_empty());
            let business_text = format!(
                "{} {} {} {} {}",
                issue.error_code,
                issue.field_path,
                issue.reason,
                issue.expected,
                issue.repair_suggestion
            )
            .to_ascii_lowercase();
            assert!(!business_text.contains("stderr"));
            assert!(!business_text.contains("sql"));
        }
    }

    #[test]
    fn unknown_optional_json_fields_are_ignored() {
        let json = r#"
        {
          "chapters": [{
            "id": "chapter-1",
            "title": "The first turn",
            "start_ms": 0,
            "end_ms": 10000,
            "mainline": "The group identifies a new problem.",
            "evidence": [{"kind": "transcript", "window_id": "window-1"}],
            "experimental_note": {"style": "free-form"}
          }],
          "future_optional_block": ["allowed to ignore"]
        }
        "#;
        let parsed = match serde_json::from_str::<ChapterAgentOutput>(json) {
            Ok(output) => output,
            Err(error) => panic!("known chapter contract should deserialize: {error}"),
        };

        let report = validate_chapter_output(&parsed, &context());

        assert!(report.is_valid());
    }
}
