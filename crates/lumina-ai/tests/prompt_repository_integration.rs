use lumina_ai::chapter::{
    validate_chapter_output, ChapterAgentOutput, ChapterOutput, ChapterValidationContext,
    EvidenceReference,
};
use lumina_ai::prompts::{
    ChapterContext, EpisodeContext, MediaContext, PromptRepository, PromptSlots,
    ScreenshotReference, SpoilerBoundary, TaskId, TranscriptLine, TranscriptWindow,
    ValidationIssue, ValidationReport, ViewingContext, MAX_VALIDATION_RETRIES,
    PROMPT_REPOSITORY_VERSION,
};

fn integration_slots() -> PromptSlots {
    PromptSlots::default()
        .with_media(MediaContext {
            media_id: "media:integration:1".to_owned(),
            title: Some("Integration fixture".to_owned()),
            duration_ms: Some(120_000),
        })
        .with_episode(EpisodeContext {
            series_id: Some("series:integration".to_owned()),
            series_title: Some("Integration series".to_owned()),
            season: Some(1),
            episode: Some(2),
            title: Some("The bounded episode".to_owned()),
        })
        .with_viewing(ViewingContext {
            position_ms: 60_000,
            spoiler_boundary: SpoilerBoundary::CurrentPosition,
        })
        .with_chapter(ChapterContext {
            chapter_id: Some("chapter:integration:1".to_owned()),
            title: Some("The opening".to_owned()),
            start_ms: Some(1_000),
            end_ms: Some(10_000),
            mainline: Some("The conflict becomes visible.".to_owned()),
        })
        .with_transcript_window(TranscriptWindow {
            window_id: "window:integration:1".to_owned(),
            start_ms: 1_000,
            end_ms: 4_000,
            lines: vec![TranscriptLine {
                start_ms: 1_000,
                end_ms: 2_000,
                text: "The evidence is introduced.".to_owned(),
            }],
        })
        .with_screenshot(ScreenshotReference {
            asset_id: "asset:integration:1".to_owned(),
            timestamp_ms: 2_000,
            resource_ref: "resource://integration/frame-1".to_owned(),
            note: Some("establishing frame".to_owned()),
        })
        .with_user_instruction("Keep the result bounded by the current viewing position.")
}

fn issue(error_code: &str, field_path: &str, marker: &str) -> ValidationIssue {
    ValidationIssue::new(
        error_code,
        field_path,
        format!("The field failed deterministic integration validation: {marker}."),
        "a non-empty value with the chapter output contract shape",
        format!("Repair only the field identified by {field_path}."),
    )
    .with_actual_value_summary(marker)
}

#[test]
fn every_versioned_repository_task_composes_from_typed_slots() {
    let repository = PromptRepository::new();
    assert_eq!(repository.task_ids().len(), 6);

    for task_id in repository.task_ids() {
        let definition = repository.definition(*task_id);
        let composed = match repository.compose(*task_id, &integration_slots()) {
            Ok(composed) => composed,
            Err(error) => panic!("task {task_id} should compose: {error}"),
        };

        assert_eq!(definition.version, PROMPT_REPOSITORY_VERSION);
        assert_eq!(composed.task_id(), *task_id);
        assert_eq!(composed.version(), PROMPT_REPOSITORY_VERSION);
        assert_eq!(composed.attempt(), 1);
        assert_eq!(composed.retry_count(), 0);
        assert!(composed
            .initial_prompt()
            .contains(&format!("Lumina task: {task_id}")));
        assert!(composed
            .initial_prompt()
            .contains(&format!("Prompt version: v{}", PROMPT_REPOSITORY_VERSION)));
        assert!(composed.initial_prompt().contains("media:integration:1"));
        assert!(composed.initial_prompt().contains("window:integration:1"));
        assert!(composed.initial_prompt().contains("asset:integration:1"));
    }
}

#[test]
fn chapter_agent_prompts_are_not_general_chat_prompts() {
    let repository = PromptRepository::new();
    let chapter_prompt = match repository.compose(TaskId::ChapterSegment, &integration_slots()) {
        Ok(prompt) => prompt,
        Err(error) => panic!("chapter prompt should compose: {error}"),
    };

    assert!(chapter_prompt.initial_prompt().contains("chapter_segment"));
    assert!(chapter_prompt
        .initial_prompt()
        .contains("transcript windows and screenshots"));
    assert!(!chapter_prompt.initial_prompt().contains("free_chat"));
    assert!(!chapter_prompt.initial_prompt().contains("chat history"));
    assert!(!chapter_prompt.initial_prompt().contains("general chat"));

    for task_id in repository.task_ids() {
        let definition = repository.definition(*task_id);
        assert!(!definition.task_id.as_str().contains("chat"));
    }
}

#[test]
fn chapter_output_validation_is_prompt_compatible_and_rejects_untrusted_evidence() {
    let output = ChapterAgentOutput {
        chapters: vec![ChapterOutput {
            id: "chapter:invalid".to_owned(),
            title: "A chapter".to_owned(),
            start_ms: 10_000,
            end_ms: 20_000,
            mainline: "A bounded mainline.".to_owned(),
            evidence: vec![EvidenceReference::transcript("window:not-registered")],
        }],
        ..ChapterAgentOutput::default()
    };
    let report = validate_chapter_output(&output, &ChapterValidationContext::new(15_000));

    assert!(!report.is_valid());
    assert!(!report.hard_error_report().issues.is_empty());
    assert!(report
        .hard_error_report()
        .issues
        .iter()
        .any(|item| item.field_path.contains("evidence")));
    assert!(report
        .hard_error_report()
        .issues
        .iter()
        .all(|item| !item.reason.contains("stderr") && !item.reason.contains("JSON-RPC")));
}

#[test]
fn validation_retries_append_only_the_current_report_and_stop_after_three_same_errors() {
    let repository = PromptRepository::new();
    let mut prompt = match repository.compose(TaskId::ChapterSegment, &integration_slots()) {
        Ok(prompt) => prompt,
        Err(error) => panic!("chapter prompt should compose: {error}"),
    };
    let initial = prompt.initial_prompt().to_owned();

    let first_report = ValidationReport::single(issue(
        "missing_mainline",
        "chapters[0].mainline",
        "first-report-only",
    ));
    let first_delta = match prompt.append_validation_report(first_report) {
        Ok(delta) => delta,
        Err(error) => panic!("first validation retry should work: {error}"),
    };
    assert!(first_delta.as_str().contains("first-report-only"));
    assert!(!first_delta.as_str().contains("media:integration:1"));
    assert!(!first_delta.as_str().contains("Lumina task:"));

    let second_report = ValidationReport::single(issue(
        "invalid_interval",
        "chapters[0].end_ms",
        "second-report-only",
    ));
    let second_delta = match prompt.append_validation_report(second_report) {
        Ok(delta) => delta,
        Err(error) => panic!("second validation retry should work: {error}"),
    };
    assert!(second_delta.as_str().contains("second-report-only"));
    assert!(!second_delta.as_str().contains("first-report-only"));
    assert!(!second_delta.as_str().contains(&initial));
    assert_eq!(second_delta.retry_count, 2);

    let mut repeated_prompt = match repository.compose(TaskId::ChapterSegment, &integration_slots())
    {
        Ok(prompt) => prompt,
        Err(error) => panic!("chapter prompt should compose: {error}"),
    };
    let repeated_error =
        ValidationReport::single(issue("same_error", "chapters[0].title", "same-error"));
    for expected_retry_count in 1..=MAX_VALIDATION_RETRIES {
        let delta = match repeated_prompt.append_validation_report(repeated_error.clone()) {
            Ok(delta) => delta,
            Err(error) => panic!("retry {expected_retry_count} should work: {error}"),
        };
        assert_eq!(delta.retry_count, expected_retry_count);
    }

    let exhausted = match repeated_prompt.append_validation_report(repeated_error) {
        Ok(_) => panic!("the fourth retry of the same error must fail"),
        Err(error) => error,
    };
    assert!(exhausted.to_string().contains("same_error"));
    assert_eq!(repeated_prompt.retry_count(), MAX_VALIDATION_RETRIES);
}

#[test]
fn invalid_validation_reports_do_not_consume_retry_budget() {
    let repository = PromptRepository::new();
    let mut prompt = match repository.compose(TaskId::ChapterRecap, &PromptSlots::default()) {
        Ok(prompt) => prompt,
        Err(error) => panic!("recap prompt should compose: {error}"),
    };

    assert!(prompt
        .append_validation_report(ValidationReport::default())
        .is_err());
    assert!(prompt
        .append_validation_report(ValidationReport::single(ValidationIssue::new(
            "", "field", "reason", "expected", "repair",
        )))
        .is_err());
    assert_eq!(prompt.attempt(), 1);
    assert_eq!(prompt.retry_count(), 0);
}
