//! Background execution for user-triggered chapter segmentation.
//!
//! Prompt rules and output validation remain in `lumina-ai`; ACP remains a
//! generic isolated client. This module only orchestrates the desktop inputs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lumina_acp::agent::workspace::resolve_session_cwd;
use lumina_acp::jobs::isolated::ChapterSession;
use lumina_acp::{AcpError, AcpErrorCode, AcpSessionModelSelection};
use lumina_ai::chapter::{
    validate_chapter_output, ChapterAgentOutput, ChapterOutput, ChapterValidationContext,
    EvidenceReference,
};
use lumina_ai::prompts::{
    compose_prompt, EpisodeContext, MediaContext, PromptSlots, SpoilerBoundary, TaskId,
    TranscriptWindow, ValidationIssue, ValidationReport, ViewingContext, MAX_VALIDATION_RETRIES,
};
use lumina_ai::{build_screenshot_reference, build_transcript_windows, ScreenshotMetadata};
use lumina_library::{
    AgentTaskRecord, Database, DatabaseError, DatabaseErrorCode, DatabaseResult,
    LegacyEpisodeMigration, NewAgentAttempt, NewChapter, NewChapterAsset, NewChapterRevision,
    NewEpisode, NewQuestionCandidate, NewSeries, NewWatchFeedItem, Repository,
};
use lumina_media::frame_capture::{
    capture_frames, detect_scene_times, select_keyframes, DEFAULT_SCENE_THRESHOLD,
};
use lumina_media::MediaInspector;
use lumina_subtitle::{SubtitleService, Transcript};
use sha2::{Digest, Sha256};

use super::{
    now_ms_for_command, open_database, ChapterCommandError, ChapterEpisodeIdentity,
    ChapterSegmentationRequest, PersistedChapterFailure,
};

const TRANSCRIPT_WINDOW_WIDTH_MS: u64 = 30_000;
const MAX_COVERAGE_POINTS: usize = 12;
const MAX_SCREENSHOTS: usize = 8;

#[derive(Debug)]
struct WorkerFailure {
    code: &'static str,
    message: &'static str,
    details: String,
    validation_report: Option<ValidationReport>,
    retry_count: i64,
}

impl WorkerFailure {
    fn business(message: &'static str, details: impl Into<String>) -> Self {
        Self {
            code: "ChapterAnalysisFailed",
            message,
            details: details.into(),
            validation_report: None,
            retry_count: 0,
        }
    }

    fn agent_not_configured(details: impl Into<String>) -> Self {
        Self {
            code: "AgentNotConfigured",
            message: "尚未配置可用的 AI Agent，请先完成 Agent 设置。",
            details: details.into(),
            validation_report: None,
            retry_count: 0,
        }
    }

    fn agent_unavailable(error: AcpError) -> Self {
        let details = error.details.clone().unwrap_or_else(|| error.to_string());
        if error.code == AcpErrorCode::NotConfigured {
            Self::agent_not_configured(details)
        } else {
            Self {
                code: "AgentUnavailable",
                message: "章节 Agent 暂时不可用，请检查 Agent 设置后重试。",
                details,
                validation_report: None,
                retry_count: 0,
            }
        }
    }

    fn validation(report: ValidationReport, retry_count: i64) -> Self {
        Self {
            code: "ValidationFailed",
            message: "章节分段结果未通过校验，请再次尝试。",
            details: serde_json::to_string(&report)
                .unwrap_or_else(|_| "validation report encoding failed".to_string()),
            validation_report: Some(report),
            retry_count,
        }
    }

    fn persisted_report(&self) -> String {
        let persisted = PersistedChapterFailure {
            code: self.code.to_string(),
            message: self.message.to_string(),
            summary: None,
            validation_report: self.validation_report.clone(),
        };
        serde_json::to_string(&persisted).unwrap_or_else(|_| self.message.to_string())
    }

    fn into_command_error(self) -> ChapterCommandError {
        tracing::error!(message = self.message, details = %self.details, "chapter worker failed");
        ChapterCommandError {
            code: self.code.to_string(),
            message: self.message.to_string(),
            details: Some(self.details),
        }
    }
}

#[derive(Debug)]
struct EvidenceBundle {
    transcript_windows: Vec<TranscriptWindow>,
    screenshots: Vec<lumina_ai::prompts::ScreenshotReference>,
}

#[derive(Debug)]
struct AttemptResult {
    output: ChapterAgentOutput,
    evidence: EvidenceBundle,
    media_path: PathBuf,
    duration_ms: u64,
    validation_retry_count: i64,
}

/// Execute one claimed task. Concurrent callers for the same task key are
/// safe because the SQLite claim is conditional and atomic.
pub(super) fn run(request: ChapterSegmentationRequest) -> Result<(), ChapterCommandError> {
    let mut database = open_database()?;
    let task_key = super::task_key(&request);
    let Some(task) = database
        .repository()
        .claim_agent_task_by_key(&task_key)
        .map_err(ChapterCommandError::storage)?
    else {
        return Ok(());
    };

    let attempt_id = database
        .repository()
        .insert_agent_attempt(&NewAgentAttempt::new(
            task.id,
            task.attempt_count,
            "chapter_segmentation",
            "running",
            task.prompt_version.clone(),
            now_ms_for_command(),
        ))
        .map_err(ChapterCommandError::storage)?;

    match execute_attempt(&request, &task) {
        Ok(result) => {
            let output_json = serde_json::to_string(&result.output).map_err(|error| {
                WorkerFailure::business("章节分段结果保存失败，请重试", error.to_string())
            });
            match output_json {
                Ok(output_json) => {
                    match database.transaction(|repository| {
                        let episode_id = ensure_episode(
                            repository,
                            &request,
                            &result.media_path,
                            result.duration_ms,
                        )?;
                        project_output(
                            repository,
                            &task,
                            episode_id,
                            &result.output,
                            &result.evidence,
                            request
                                .spoiler_boundary
                                .unwrap_or(SpoilerBoundary::FullMedia),
                        )?;
                        if !repository.update_agent_task_scope(task.id, Some(episode_id), None)? {
                            return Err(persistence_error("agent task disappeared"));
                        }
                        if !repository.complete_agent_task(
                            task.id,
                            "succeeded",
                            task.attempt_count,
                            result.validation_retry_count,
                            None,
                            &output_json,
                        )? {
                            return Err(persistence_error("agent task could not be completed"));
                        }
                        repository.update_agent_attempt_status(
                            attempt_id,
                            "succeeded",
                            None,
                            now_ms_for_command(),
                        )?;
                        Ok(())
                    }) {
                        Ok(()) => Ok(()),
                        Err(error) => finish_failed(
                            &database,
                            &task,
                            attempt_id,
                            WorkerFailure::business(
                                "章节结果写入失败，请重试",
                                error.details.clone().unwrap_or(error.message),
                            ),
                        ),
                    }
                }
                Err(error) => finish_failed(&database, &task, attempt_id, error),
            }
        }
        Err(error) => finish_failed(&database, &task, attempt_id, error),
    }
}

fn finish_failed(
    database: &Database,
    task: &AgentTaskRecord,
    attempt_id: i64,
    failure: WorkerFailure,
) -> Result<(), ChapterCommandError> {
    let persisted_report = failure.persisted_report();
    let status = failure_status(task.attempt_count, task.max_attempts);
    database
        .repository()
        .update_agent_task_status(
            task.id,
            status,
            task.attempt_count,
            failure.retry_count,
            Some(&persisted_report),
        )
        .map_err(ChapterCommandError::storage)?;
    finish_attempt(
        &database.repository(),
        attempt_id,
        "failed",
        Some(&persisted_report),
    )?;
    Err(failure.into_command_error())
}

fn failure_status(attempt_count: i64, max_attempts: i64) -> &'static str {
    if attempt_count < max_attempts {
        "validation_failure"
    } else {
        "failed"
    }
}

fn finish_attempt(
    repository: &Repository<'_>,
    attempt_id: i64,
    status: &str,
    validation_report: Option<&str>,
) -> Result<(), ChapterCommandError> {
    repository
        .update_agent_attempt_status(attempt_id, status, validation_report, now_ms_for_command())
        .map_err(ChapterCommandError::storage)?;
    Ok(())
}

fn persistence_error(details: impl Into<String>) -> DatabaseError {
    DatabaseError {
        code: DatabaseErrorCode::QueryFailed,
        message: "应用数据存储写入失败，请重试".to_string(),
        details: Some(details.into()),
    }
}

fn ensure_episode(
    repository: &Repository<'_>,
    request: &ChapterSegmentationRequest,
    media_path: &Path,
    duration_ms: u64,
) -> DatabaseResult<i64> {
    let fallback_title = media_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(request.episode_key.trim())
        .to_string();
    if let Some((
        series_stable_id,
        episode_stable_id,
        season,
        episode_number,
        series_title,
        episode_title,
    )) = request
        .episode_identity
        .as_ref()
        .and_then(ChapterEpisodeIdentity::authoritative_parts)
    {
        let series_title = series_title.unwrap_or(fallback_title.as_str());
        let series_id = match repository.get_series_by_stable_id(series_stable_id)? {
            Some(series) => series.id,
            None => {
                repository.insert_series(&NewSeries::new(series_stable_id, series_title, "tmdb"))?
            }
        };
        let mut new_episode = NewEpisode::new(series_id, episode_stable_id, "tmdb");
        new_episode.season_number = Some(i64::from(season));
        new_episode.episode_number = Some(i64::from(episode_number));
        new_episode.title = Some(episode_title.unwrap_or(fallback_title.as_str()).to_string());
        new_episode.duration_ms = Some(i64_from_ms(duration_ms)?);
        let episode = repository.get_or_create_episode(&new_episode)?;

        if let Some(legacy_episode_id) = find_legacy_episode_for_migration(repository, request)? {
            let migration = repository.migrate_legacy_episode(&LegacyEpisodeMigration::new(
                legacy_episode_id,
                episode.id,
            ))?;
            if migration.status == lumina_library::EpisodeMigrationStatus::Conflict {
                tracing::warn!(
                    legacy_episode_id,
                    authoritative_episode_id = episode.id,
                    diagnostics = ?migration.diagnostics,
                    "legacy episode migration retained both sources"
                );
            }
        }
        return Ok(episode.id);
    }

    let series_stable_id = format!("media-series:{}", request.media_path.trim());
    let series_id = match repository.get_series_by_stable_id(&series_stable_id)? {
        Some(series) => series.id,
        None => repository.insert_series(&NewSeries::new(
            series_stable_id,
            fallback_title.clone(),
            "local",
        ))?,
    };
    let mut new_episode = NewEpisode::new(series_id, request.episode_key.trim(), "local");
    new_episode.title = Some(fallback_title);
    new_episode.duration_ms = Some(i64_from_ms(duration_ms)?);
    repository
        .get_or_create_episode(&new_episode)
        .map(|episode| episode.id)
}

/// Find the legacy episode without guessing from a filename or changing the
/// requested identity.  Older rows used either the media path or, in an
/// earlier worker version, the caller-provided episode key as the episode
/// stable id.  Authoritative requests use `sXXeYY`, so the path lookup must be
/// attempted first.
fn find_legacy_episode_for_migration(
    repository: &Repository<'_>,
    request: &ChapterSegmentationRequest,
) -> DatabaseResult<Option<i64>> {
    let media_path = request.media_path.trim();
    let episode_key = request.episode_key.trim();
    let legacy_series_stable_id = format!("media-series:{media_path}");
    let Some(legacy_series) = repository.get_series_by_stable_id(&legacy_series_stable_id)? else {
        return Ok(None);
    };

    if let Some(legacy_episode) =
        repository.get_episode_by_stable_id(legacy_series.id, media_path)?
    {
        return Ok(Some(legacy_episode.id));
    }
    if episode_key.is_empty() || episode_key == media_path {
        return Ok(None);
    }
    repository
        .get_episode_by_stable_id(legacy_series.id, episode_key)
        .map(|episode| episode.map(|episode| episode.id))
}

fn project_output(
    repository: &Repository<'_>,
    task: &AgentTaskRecord,
    episode_id: i64,
    output: &ChapterAgentOutput,
    evidence: &EvidenceBundle,
    boundary: SpoilerBoundary,
) -> DatabaseResult<()> {
    let spoiler_level = spoiler_level(boundary);
    for chapter in &output.chapters {
        persist_chapter(
            repository,
            task,
            episode_id,
            chapter,
            evidence,
            spoiler_level,
        )?;
    }

    if let Some(recap) = output.recap.as_deref().and_then(non_empty) {
        insert_feed_item(
            repository,
            episode_id,
            None,
            None,
            task.id,
            FeedItemInput {
                item_type: "recap",
                content: &recap,
                spoiler_level,
                dedupe_key: &format!("task:{}:recap", task.id),
                content_version: &task.prompt_version,
            },
        )?;
    }
    if let Some(outlook) = output.outlook.as_deref().and_then(non_empty) {
        insert_feed_item(
            repository,
            episode_id,
            None,
            None,
            task.id,
            FeedItemInput {
                item_type: "outlook",
                content: &outlook,
                spoiler_level,
                dedupe_key: &format!("task:{}:outlook", task.id),
                content_version: &task.prompt_version,
            },
        )?;
    }
    for (index, point) in output
        .watch_points
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|point| non_empty(point))
        .enumerate()
    {
        insert_feed_item(
            repository,
            episode_id,
            None,
            None,
            task.id,
            FeedItemInput {
                item_type: "watch_point",
                content: &point,
                spoiler_level,
                dedupe_key: &format!("task:{}:watch-point:{index}", task.id),
                content_version: &task.prompt_version,
            },
        )?;
    }
    for (index, question) in output
        .questions
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|question| non_empty(question))
        .enumerate()
    {
        let fingerprint = format!("task:{}:question:{index}", task.id);
        let mut candidate =
            NewQuestionCandidate::new(&question, "ai", spoiler_level, fingerprint.clone());
        candidate.episode_id = Some(episode_id);
        candidate.task_id = Some(task.id);
        candidate.batch_key = Some(task.task_key.clone());
        repository.insert_question_candidate(&candidate)?;
        insert_feed_item(
            repository,
            episode_id,
            None,
            None,
            task.id,
            FeedItemInput {
                item_type: "question",
                content: &question,
                spoiler_level,
                dedupe_key: &format!("task:{}:question-feed:{index}", task.id),
                content_version: &task.prompt_version,
            },
        )?;
    }
    Ok(())
}

fn persist_chapter(
    repository: &Repository<'_>,
    task: &AgentTaskRecord,
    episode_id: i64,
    chapter: &ChapterOutput,
    evidence: &EvidenceBundle,
    spoiler_level: &str,
) -> DatabaseResult<i64> {
    let mut input = NewChapter::new(
        episode_id,
        chapter.id.clone(),
        i64_from_ms(chapter.start_ms)?,
        i64_from_ms(chapter.end_ms)?,
        "ai",
    );
    input.spoiler_level = spoiler_level.to_string();
    input.title = Some(chapter.title.trim().to_string());
    input.mainline = Some(chapter.mainline.trim().to_string());
    input.status = "ready".to_string();
    let chapter_id = repository.insert_chapter(&input)?;

    let revision_content = serde_json::to_string(chapter)
        .map_err(|error| persistence_error(format!("serialize chapter revision: {error}")))?;
    let mut revision = NewChapterRevision::new(
        chapter_id,
        1,
        "generated",
        revision_content,
        "ai",
        task.prompt_version.clone(),
    );
    revision.status = "accepted".to_string();
    let revision_id = repository.insert_chapter_revision(&revision)?;

    let mut screenshot_ids = BTreeSet::new();
    for reference in &chapter.evidence {
        let EvidenceReference::Screenshot { asset_id } = reference else {
            continue;
        };
        if !screenshot_ids.insert(asset_id.clone()) {
            continue;
        }
        let Some(screenshot) = evidence
            .screenshots
            .iter()
            .find(|screenshot| screenshot.asset_id == *asset_id)
        else {
            continue;
        };
        let mut asset = NewChapterAsset::new(
            chapter_id,
            "screenshot",
            screenshot.resource_ref.clone(),
            screenshot_content_hash(&screenshot.resource_ref),
            i64_from_ms(screenshot.timestamp_ms)?,
            "ai",
        );
        asset.width = None;
        asset.height = None;
        repository.insert_chapter_asset(&asset)?;
    }

    if let Some(mainline) = non_empty(chapter.mainline.as_str()) {
        insert_feed_item(
            repository,
            episode_id,
            Some(chapter_id),
            Some(revision_id),
            task.id,
            FeedItemInput {
                item_type: "chapter",
                content: &mainline,
                spoiler_level,
                dedupe_key: &format!("task:{}:chapter-feed:{}", task.id, chapter.id),
                content_version: &task.prompt_version,
            },
        )?;
    }
    Ok(chapter_id)
}

struct FeedItemInput<'a> {
    item_type: &'a str,
    content: &'a str,
    spoiler_level: &'a str,
    dedupe_key: &'a str,
    content_version: &'a str,
}

fn insert_feed_item(
    repository: &Repository<'_>,
    episode_id: i64,
    chapter_id: Option<i64>,
    revision_id: Option<i64>,
    task_id: i64,
    input: FeedItemInput<'_>,
) -> DatabaseResult<i64> {
    let mut item = NewWatchFeedItem::new(
        input.item_type,
        "ai",
        input.content,
        input.spoiler_level,
        input.content_version,
        input.dedupe_key,
    );
    item.episode_id = Some(episode_id);
    item.chapter_id = chapter_id;
    item.revision_id = revision_id;
    item.task_id = Some(task_id);
    item.published_at_ms = Some(now_ms_for_command());
    repository.insert_watch_feed_item(&item)
}

fn i64_from_ms(value: u64) -> DatabaseResult<i64> {
    i64::try_from(value).map_err(|_| persistence_error("timestamp exceeds SQLite range"))
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn spoiler_level(boundary: SpoilerBoundary) -> &'static str {
    match boundary {
        SpoilerBoundary::CurrentPosition => "current_position",
        SpoilerBoundary::CurrentChapter => "current_chapter",
        SpoilerBoundary::FullMedia => "full_media",
    }
}

fn screenshot_content_hash(path: &str) -> String {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(path, details = %error, "chapter screenshot could not be read for hashing");
            path.as_bytes().to_vec()
        }
    };
    format!("{:x}", Sha256::digest(bytes))
}

fn execute_attempt(
    request: &ChapterSegmentationRequest,
    task: &AgentTaskRecord,
) -> Result<AttemptResult, WorkerFailure> {
    let media_path = local_media_path(request)?;
    let media_info = MediaInspector::inspect(&media_path).map_err(|error| {
        WorkerFailure::business("无法读取媒体信息，请检查媒体文件", error.to_string())
    })?;
    let duration_ms = media_info
        .duration_ms
        .filter(|duration| *duration > 0)
        .ok_or_else(|| {
            WorkerFailure::business(
                "无法读取媒体时长，暂时不能生成章节",
                "media duration missing",
            )
        })?;

    let evidence = collect_evidence(request, task, &media_path, duration_ms)?;
    if evidence.transcript_windows.is_empty() && evidence.screenshots.is_empty() {
        return Err(WorkerFailure::business(
            "未找到可用于章节分析的字幕或画面证据",
            "evidence bundle is empty",
        ));
    }

    let boundary = request
        .spoiler_boundary
        .unwrap_or(SpoilerBoundary::FullMedia);
    let position_ms = request.position_ms.unwrap_or(duration_ms).min(duration_ms);
    let slots = build_prompt_slots(
        request,
        &media_path,
        duration_ms,
        position_ms,
        boundary,
        &evidence,
    );
    let validation_context =
        build_validation_context(duration_ms, position_ms, boundary, &evidence);
    let mut composed = compose_prompt(TaskId::ChapterSegment, &slots).map_err(|error| {
        WorkerFailure::business("章节分析提示词准备失败，请重试", error.to_string())
    })?;

    let profile_id = request
        .profile_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| WorkerFailure::agent_not_configured("missing profile id"))?;
    let profiles = request
        .profiles
        .clone()
        .ok_or_else(|| WorkerFailure::agent_not_configured("missing profiles"))?;
    let cwd = resolve_session_cwd(Some(&request.media_path)).map_err(|error| {
        WorkerFailure::business("章节 Agent 工作区不可用，请重试", error.to_string())
    })?;
    let session = ChapterSession::new(
        Some(cwd.to_string_lossy().into_owned()),
        profile_id.to_string(),
        profiles,
        model_selection(request),
        Some(format!("chapter-{}", task.id)),
    );

    let mut response = session
        .prompt(composed.initial_prompt())
        .map_err(WorkerFailure::agent_unavailable)?;
    let mut retry_count = 0_i64;

    loop {
        let parsed = match parse_chapter_output(&response) {
            Ok(output) => output,
            Err(report) => {
                if retry_count >= i64::from(MAX_VALIDATION_RETRIES) {
                    return Err(WorkerFailure::validation(report, retry_count));
                }
                let delta = composed
                    .append_validation_report(report.clone())
                    .map_err(|_| WorkerFailure::validation(report.clone(), retry_count))?;
                retry_count += 1;
                response = session
                    .prompt(delta.message)
                    .map_err(WorkerFailure::agent_unavailable)?;
                continue;
            }
        };

        let report = validate_chapter_output(&parsed, &validation_context);
        if report.is_valid() {
            return Ok(AttemptResult {
                output: parsed,
                evidence,
                media_path,
                duration_ms,
                validation_retry_count: retry_count,
            });
        }
        if retry_count >= i64::from(MAX_VALIDATION_RETRIES) {
            return Err(WorkerFailure::validation(
                report.hard_error_report().clone(),
                retry_count,
            ));
        }
        let delta = composed
            .append_validation_report(report.hard_error_report().clone())
            .map_err(|_| {
                WorkerFailure::validation(report.hard_error_report().clone(), retry_count)
            })?;
        retry_count += 1;
        response = session
            .prompt(delta.message)
            .map_err(WorkerFailure::agent_unavailable)?;
    }
}

fn local_media_path(request: &ChapterSegmentationRequest) -> Result<PathBuf, WorkerFailure> {
    let path_text = request.media_path.trim();
    if path_text.is_empty() || path_text.starts_with("http://") || path_text.starts_with("https://")
    {
        return Err(WorkerFailure::business(
            "当前媒体不支持 AI 分段",
            "chapter segmentation requires a local media file",
        ));
    }
    let path = PathBuf::from(path_text);
    if !path.is_file() {
        return Err(WorkerFailure::business(
            "媒体文件不可用，请重新打开后重试",
            "media file missing",
        ));
    }
    Ok(path)
}

fn collect_evidence(
    request: &ChapterSegmentationRequest,
    task: &AgentTaskRecord,
    media_path: &Path,
    duration_ms: u64,
) -> Result<EvidenceBundle, WorkerFailure> {
    let transcript = load_transcript(request, media_path)?;
    let transcript_windows = transcript
        .as_ref()
        .map(|value| build_transcript_windows(&value.cues, TRANSCRIPT_WINDOW_WIDTH_MS))
        .transpose()
        .map_err(|error| WorkerFailure::business("字幕证据准备失败，请重试", error.to_string()))?
        .unwrap_or_default();
    let screenshots = collect_screenshots(task, media_path, duration_ms)?;
    Ok(EvidenceBundle {
        transcript_windows,
        screenshots,
    })
}

fn load_transcript(
    request: &ChapterSegmentationRequest,
    media_path: &Path,
) -> Result<Option<Transcript>, WorkerFailure> {
    let choices = match SubtitleService::list_choices(media_path) {
        Ok(choices) => choices,
        Err(error) => {
            tracing::warn!(details = %error, "chapter worker could not list subtitles");
            return Ok(None);
        }
    };
    let choice = if let Some(choice_id) = request.subtitle_choice_id.as_deref() {
        choices.iter().find(|candidate| candidate.id == choice_id)
    } else {
        choices.iter().find(|candidate| candidate.supported)
    };
    let Some(choice) = choice else {
        return Ok(None);
    };
    if !choice.supported {
        return Err(WorkerFailure::business(
            "当前字幕格式无法用于章节分析",
            "selected subtitle is not supported",
        ));
    }
    SubtitleService::load_choice(media_path, &choice.id)
        .map(Some)
        .map_err(|error| WorkerFailure::business("字幕证据准备失败，请重试", error.to_string()))
}

fn collect_screenshots(
    task: &AgentTaskRecord,
    media_path: &Path,
    duration_ms: u64,
) -> Result<Vec<lumina_ai::prompts::ScreenshotReference>, WorkerFailure> {
    let coverage = coverage_times(duration_ms);
    let scenes = detect_scene_times(media_path, DEFAULT_SCENE_THRESHOLD).unwrap_or_default();
    let selected = select_keyframes(&coverage, &scenes, MAX_SCREENSHOTS);
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let root = media_path.parent().unwrap_or_else(|| Path::new("."));
    let output_dir = root
        .join(".lumina")
        .join("tmp")
        .join(format!("chapter-capture-{}", task.id));
    let paths = capture_frames(media_path, &selected, &output_dir)
        .map_err(|error| WorkerFailure::business("画面证据准备失败，请重试", error.to_string()))?;
    let mut references = Vec::new();
    for (index, (path, time_sec)) in paths.into_iter().zip(selected).enumerate() {
        let timestamp_ms = (time_sec.max(0.0) * 1000.0).round() as u64;
        let metadata = ScreenshotMetadata {
            asset_id: format!("chapter-{}-frame-{index:02}", task.id),
            timestamp_ms,
            resource_ref: path.to_string_lossy().into_owned(),
            note: Some("worker representative frame".to_string()),
            media_duration_ms: Some(duration_ms),
        };
        match build_screenshot_reference(metadata) {
            Ok(reference) => references.push(reference),
            Err(error) => tracing::warn!(details = %error, "invalid chapter screenshot metadata"),
        }
    }
    Ok(references)
}

fn coverage_times(duration_ms: u64) -> Vec<f64> {
    if duration_ms == 0 {
        return Vec::new();
    }
    let points = MAX_COVERAGE_POINTS.min(((duration_ms / 30_000) as usize).saturating_add(1));
    let points = points.max(1);
    if points == 1 {
        return vec![0.0];
    }
    let last_sec = (duration_ms.saturating_sub(100) as f64) / 1000.0;
    (0..points)
        .map(|index| last_sec * index as f64 / (points - 1) as f64)
        .collect()
}

fn build_prompt_slots(
    request: &ChapterSegmentationRequest,
    media_path: &Path,
    duration_ms: u64,
    position_ms: u64,
    boundary: SpoilerBoundary,
    evidence: &EvidenceBundle,
) -> PromptSlots {
    let identity = request
        .episode_identity
        .as_ref()
        .and_then(ChapterEpisodeIdentity::authoritative_parts);
    let title = media_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_string)
        .or_else(|| {
            request
                .episode_key
                .trim()
                .is_empty()
                .then(|| request.episode_key.clone())
        });
    let (media_id, series_id, series_title, season, episode, episode_title) = identity
        .map(
            |(
                series_stable_id,
                episode_stable_id,
                season,
                episode,
                series_title,
                episode_title,
            )| {
                (
                    episode_stable_id.to_string(),
                    Some(series_stable_id.to_string()),
                    series_title.map(str::to_string),
                    Some(season),
                    Some(episode),
                    episode_title.map(str::to_string),
                )
            },
        )
        .unwrap_or_else(|| (request.episode_key.clone(), None, None, None, None, None));
    let mut slots = PromptSlots::default()
        .with_media(MediaContext {
            media_id,
            title,
            duration_ms: Some(duration_ms),
        })
        .with_episode(EpisodeContext {
            series_id,
            series_title,
            season,
            episode,
            title: episode_title.or_else(|| Some(request.episode_key.clone())),
        })
        .with_viewing(ViewingContext {
            position_ms,
            spoiler_boundary: boundary,
        });
    for window in &evidence.transcript_windows {
        slots = slots.with_transcript_window(window.clone());
    }
    for screenshot in &evidence.screenshots {
        slots = slots.with_screenshot(screenshot.clone());
    }
    slots
}

fn build_validation_context(
    duration_ms: u64,
    position_ms: u64,
    boundary: SpoilerBoundary,
    evidence: &EvidenceBundle,
) -> ChapterValidationContext {
    let mut context = ChapterValidationContext::new(duration_ms).with_viewing(ViewingContext {
        position_ms,
        spoiler_boundary: boundary,
    });
    for window in &evidence.transcript_windows {
        context = context.with_transcript_window(window.clone());
    }
    for screenshot in &evidence.screenshots {
        context = context.with_screenshot(screenshot.clone());
    }
    context
}

fn model_selection(request: &ChapterSegmentationRequest) -> Option<AcpSessionModelSelection> {
    let model_id = request.model_id.as_deref()?.trim();
    if model_id.is_empty() {
        return None;
    }
    Some(AcpSessionModelSelection {
        model_id: model_id.to_string(),
        reasoning_effort: request
            .reasoning_effort
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    })
}

fn parse_chapter_output(raw: &str) -> Result<ChapterAgentOutput, ValidationReport> {
    let trimmed = raw.trim();
    let candidate = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .or_else(|| {
            trimmed
                .strip_prefix("```")
                .and_then(|value| value.strip_suffix("```"))
        })
        .map(str::trim)
        .unwrap_or(trimmed);
    serde_json::from_str(candidate).map_err(|_| {
        ValidationReport::single(
            ValidationIssue::new(
                "invalid_structured_output",
                "output",
                "The Agent response could not be parsed as the chapter output contract.",
                "valid JSON matching chapter_segment.v1",
                "Return only the structured chapter result as JSON.",
            )
            .with_actual_value_summary("non-JSON or malformed JSON response"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumina_library::{Database, NewEpisode, NewSeries};

    #[test]
    fn parses_plain_json_and_json_fence() {
        assert!(parse_chapter_output(r#"{"chapters":[]}"#).is_ok());
        assert!(parse_chapter_output("```json\n{\"chapters\":[]}\n```").is_ok());
    }

    #[test]
    fn malformed_output_becomes_business_validation_report() {
        let report = parse_chapter_output("not json").expect_err("must fail");
        assert_eq!(report.issues[0].error_code, "invalid_structured_output");
        assert!(!report.issues[0].reason.contains("stderr"));
    }

    #[test]
    fn coverage_is_bounded_and_deterministic() {
        let points = coverage_times(600_000);
        assert!(points.len() <= MAX_COVERAGE_POINTS);
        assert_eq!(points.first().copied(), Some(0.0));
        assert!(points.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn failure_is_retryable_until_task_attempt_budget_is_exhausted() {
        assert_eq!(failure_status(1, 3), "validation_failure");
        assert_eq!(failure_status(2, 3), "validation_failure");
        assert_eq!(failure_status(3, 3), "failed");
    }

    #[test]
    fn persisted_failure_contains_business_fields_only() {
        let failure = WorkerFailure::agent_not_configured("private agent details");
        let report = failure.persisted_report();
        assert!(report.contains("AgentNotConfigured"));
        assert!(report.contains("尚未配置可用的 AI Agent"));
        assert!(!report.contains("private agent details"));
        assert!(!report.contains("stderr"));
    }

    #[test]
    fn legacy_lookup_prefers_media_path_when_authoritative_episode_key_differs(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::open_in_memory()?;
        let repository = database.repository();
        let media_path = r"C:\media\show\episode.mkv";
        let series_id = repository.insert_series(&NewSeries::new(
            format!("media-series:{media_path}"),
            "Legacy Show",
            "local",
        ))?;
        let legacy_episode =
            repository.insert_episode(&NewEpisode::new(series_id, media_path, "local"))?;
        let request = ChapterSegmentationRequest {
            media_path: media_path.to_string(),
            episode_key: "s01e01".to_string(),
            episode_identity: Some(ChapterEpisodeIdentity::Authoritative {
                series_stable_id: "tmdb:tv:42".to_string(),
                episode_stable_id: "s01e01".to_string(),
                season: 1,
                episode: 1,
                series_title: None,
                title: None,
            }),
            profile_id: None,
            profiles: None,
            model_id: None,
            reasoning_effort: None,
            subtitle_choice_id: None,
            position_ms: None,
            spoiler_boundary: None,
        };

        assert_eq!(
            find_legacy_episode_for_migration(&repository, &request)?,
            Some(legacy_episode)
        );
        Ok(())
    }

    #[test]
    fn legacy_lookup_falls_back_to_old_episode_key_only_after_path_miss(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::open_in_memory()?;
        let repository = database.repository();
        let media_path = r"C:\media\show\episode.mkv";
        let series_id = repository.insert_series(&NewSeries::new(
            format!("media-series:{media_path}"),
            "Legacy Show",
            "local",
        ))?;
        let legacy_episode =
            repository.insert_episode(&NewEpisode::new(series_id, "s01e01", "local"))?;
        let request = ChapterSegmentationRequest {
            media_path: media_path.to_string(),
            episode_key: "s01e01".to_string(),
            episode_identity: None,
            profile_id: None,
            profiles: None,
            model_id: None,
            reasoning_effort: None,
            subtitle_choice_id: None,
            position_ms: None,
            spoiler_boundary: None,
        };

        assert_eq!(
            find_legacy_episode_for_migration(&repository, &request)?,
            Some(legacy_episode)
        );
        Ok(())
    }
}
