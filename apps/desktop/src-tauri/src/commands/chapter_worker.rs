//! Background execution for user-triggered chapter segmentation.
//!
//! Prompt rules and output validation remain in `lumina-ai`; ACP remains a
//! generic isolated client. This module only orchestrates the desktop inputs.

use std::path::{Path, PathBuf};

use lumina_acp::agent::workspace::resolve_session_cwd;
use lumina_acp::jobs::isolated::ChapterSession;
use lumina_acp::{AcpError, AcpErrorCode, AcpSessionModelSelection};
use lumina_ai::prompts::{
    compose_prompt, EpisodeContext, MediaContext, PromptSlots, SpoilerBoundary, TaskId,
    TranscriptWindow, ValidationReport, ViewingContext,
};
use lumina_ai::{build_screenshot_reference, build_transcript_windows, ScreenshotMetadata};
use lumina_library::{
    AgentTaskRecord, Database, DatabaseError, DatabaseErrorCode, DatabaseResult,
    LegacyEpisodeMigration, NewAgentAttempt, NewEpisode, NewSeries, Repository,
};
use lumina_media::frame_capture::{
    capture_frames, detect_scene_times, select_keyframes, DEFAULT_SCENE_THRESHOLD,
};
use lumina_media::MediaInspector;
use lumina_subtitle::{SubtitleService, Transcript};

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
    /// The final assistant text is diagnostic data only. The scoped chapter
    /// session never parses it; durable task state is authoritative.
    assistant_response: String,
}

#[derive(Debug)]
struct AttemptScope {
    media_path: PathBuf,
    duration_ms: u64,
    episode_id: i64,
    database_path: PathBuf,
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

    let scope = match establish_attempt_scope(&mut database, &request, &task) {
        Ok(scope) => scope,
        Err(error) => return finish_failed(&database, &task, attempt_id, error),
    };

    match execute_attempt(&request, &task, attempt_id, &scope) {
        Ok(result) => settle_attempt(&mut database, &task, attempt_id, result),
        Err(error) => finish_failed(&database, &task, attempt_id, error),
    }
}

/// Establish the complete task scope before the ACP process is spawned.
/// Episode creation and task linking are both idempotent repository operations
/// and occur in one transaction so the chapter snapshot never points at a
/// half-created episode/task pair.
fn establish_attempt_scope(
    database: &mut Database,
    request: &ChapterSegmentationRequest,
    task: &AgentTaskRecord,
) -> Result<AttemptScope, WorkerFailure> {
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
    let episode_id = database
        .transaction(|repository| {
            let episode_id = ensure_episode(repository, request, &media_path, duration_ms)?;
            if !repository.update_agent_task_scope(task.id, Some(episode_id), None)? {
                return Err(persistence_error(
                    "agent task disappeared while establishing scope",
                ));
            }
            Ok(episode_id)
        })
        .map_err(|error| {
            WorkerFailure::business("章节任务范围初始化失败，请重试", error.message)
        })?;
    let database_path = super::database_path().map_err(|error| {
        WorkerFailure::business(
            "章节任务范围初始化失败，请重试",
            error.details.unwrap_or(error.message),
        )
    })?;
    Ok(AttemptScope {
        media_path,
        duration_ms,
        episode_id,
        database_path,
    })
}

fn settle_attempt(
    database: &mut Database,
    task: &AgentTaskRecord,
    attempt_id: i64,
    result: AttemptResult,
) -> Result<(), ChapterCommandError> {
    // Keep the response available for diagnostics, but never use assistant
    // text as a completion signal for a scoped Batch G tool session.
    let _assistant_response = result.assistant_response;
    let persisted = database
        .repository()
        .get_agent_task(task.id)
        .map_err(ChapterCommandError::storage)?
        .ok_or_else(|| {
            ChapterCommandError::internal("chapter task disappeared after Agent session")
        })?;

    match persisted.status.as_str() {
        // The finalize tool owns publication and the terminal task/attempt
        // update.  Do not infer success from the assistant response.
        "succeeded" => Ok(()),
        _ => finish_failed(
            database,
            task,
            attempt_id,
            WorkerFailure::business(
                "章节 Agent 未完成章节写入，请再次尝试。",
                "scoped chapter session ended without durable finalize",
            ),
        ),
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

fn i64_from_ms(value: u64) -> DatabaseResult<i64> {
    i64::try_from(value).map_err(|_| persistence_error("timestamp exceeds SQLite range"))
}

fn spoiler_level(boundary: SpoilerBoundary) -> &'static str {
    match boundary {
        SpoilerBoundary::CurrentPosition => "current_position",
        SpoilerBoundary::CurrentChapter => "current_chapter",
        SpoilerBoundary::FullMedia => "full_media",
    }
}

fn execute_attempt(
    request: &ChapterSegmentationRequest,
    task: &AgentTaskRecord,
    attempt_id: i64,
    scope: &AttemptScope,
) -> Result<AttemptResult, WorkerFailure> {
    let boundary = request
        .spoiler_boundary
        .unwrap_or(SpoilerBoundary::FullMedia);
    let position_ms = request
        .position_ms
        .unwrap_or(scope.duration_ms)
        .min(scope.duration_ms);
    let cwd = resolve_session_cwd(Some(&request.media_path)).map_err(|error| {
        WorkerFailure::business("章节 Agent 工作区不可用，请重试", error.to_string())
    })?;
    crate::acp::adapter::write_chapter_task_snapshot(
        &cwd,
        &scope.media_path,
        position_ms,
        scope.duration_ms,
        request.subtitle_choice_id.as_deref(),
        crate::mcp::ChapterTaskContext {
            task_id: task.id,
            attempt_id,
            episode_id: scope.episode_id,
            database_path: scope.database_path.to_string_lossy().into_owned(),
            media_path: scope.media_path.to_string_lossy().into_owned(),
            duration_ms: scope.duration_ms,
            spoiler_boundary: spoiler_level(boundary).to_string(),
            prompt_version: task.prompt_version.clone(),
        },
    )
    .map_err(WorkerFailure::agent_unavailable)?;

    // Prewarming is only a prompt optimization.  The isolated session has
    // task-scoped MCP read/write tools and must still start when both local
    // prewarm paths are unavailable.
    let evidence = collect_evidence(request, task, &scope.media_path, scope.duration_ms)?;

    let slots = build_prompt_slots(
        request,
        &scope.media_path,
        scope.duration_ms,
        position_ms,
        boundary,
        &evidence,
    );
    let composed = compose_prompt(TaskId::ChapterSegment, &slots).map_err(|error| {
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
    let session = ChapterSession::new(
        Some(cwd.to_string_lossy().into_owned()),
        profile_id.to_string(),
        profiles,
        model_selection(request),
        Some(format!("chapter-{}", task.id)),
    );

    let response = session
        .prompt(composed.initial_prompt())
        .map_err(WorkerFailure::agent_unavailable)?;
    Ok(AttemptResult {
        assistant_response: response,
    })
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
    let transcript_windows = match load_transcript(request, media_path) {
        Ok(Some(transcript)) => {
            match build_transcript_windows(&transcript.cues, TRANSCRIPT_WINDOW_WIDTH_MS) {
                Ok(windows) => windows,
                Err(error) => {
                    tracing::warn!(
                        details = %error,
                        "chapter worker could not build transcript evidence"
                    );
                    Vec::new()
                }
            }
        }
        Ok(None) => Vec::new(),
        Err(error) => {
            tracing::warn!(
                details = %error.details,
                "chapter worker subtitle prewarm failed; agent may use MCP transcript tools"
            );
            Vec::new()
        }
    };
    let screenshots = match collect_screenshots(task, media_path, duration_ms) {
        Ok(screenshots) => screenshots,
        Err(error) => {
            tracing::warn!(
                details = %error.details,
                "chapter worker screenshot prewarm failed; agent may use MCP capture"
            );
            Vec::new()
        }
    };
    Ok(EvidenceBundle {
        transcript_windows,
        screenshots,
    })
}

fn load_transcript(
    request: &ChapterSegmentationRequest,
    media_path: &Path,
) -> Result<Option<Transcript>, WorkerFailure> {
    if let Some(choice_id) = request
        .subtitle_choice_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if lumina_ytdl::provider::parse_cache_choice(choice_id).is_some() {
            return lumina_ytdl::provider::load_cached_choice(
                &media_path.to_string_lossy(),
                choice_id,
            )
            .map(Some)
            .map_err(|error| {
                WorkerFailure::business("字幕证据准备失败，请重试", error.to_string())
            });
        }
    }
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
        // Chapter evidence is referenced by the prompt and persisted as a
        // chapter asset. Keep it outside MCP's ephemeral tmp directory:
        // ACP snapshot synchronization clears `.lumina/tmp` before the
        // chapter session starts.
        .join("chapter-evidence")
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

#[cfg(test)]
mod tests {
    use super::*;
    use lumina_library::{
        Database, NewAgentAttempt, NewAgentTask, NewChapter, NewEpisode, NewSeries,
    };

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
    fn business_failure_exhausts_three_independent_attempts_without_publishing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::open_in_memory()?;
        let task_key = "chapter-retry-state-machine";
        let series_id = database.repository().insert_series(&NewSeries::new(
            "series:retry-state-machine",
            "Retry State Machine",
            "test",
        ))?;
        let episode_id = database.repository().insert_episode(&NewEpisode::new(
            series_id,
            "episode:retry-state-machine",
            "test",
        ))?;
        let mut task_input = NewAgentTask::new(task_key, "chapter_segmentation", "chapter.v1");
        task_input.episode_id = Some(episode_id);
        let task = database
            .repository()
            .get_or_create_agent_task(&task_input)?;
        let chapter_id = database.repository().insert_chapter(&NewChapter::new(
            episode_id,
            "draft:opening",
            0,
            1_000,
            "ai",
        ))?;
        database
            .repository()
            .ensure_agent_task_chapter_scope(task.id, episode_id, chapter_id)?;

        let expected_task_statuses = ["validation_failure", "validation_failure", "failed"];
        let mut attempt_ids = Vec::new();
        for (index, expected_status) in expected_task_statuses.iter().enumerate() {
            let attempt_number = i64::try_from(index + 1)?;
            let claimed = database
                .repository()
                .claim_agent_task_by_key(task_key)?
                .ok_or("retryable chapter task was not claimed")?;
            assert_eq!(claimed.attempt_count, attempt_number);

            let attempt_id = database
                .repository()
                .insert_agent_attempt(&NewAgentAttempt::new(
                    claimed.id,
                    attempt_number,
                    "chapter_segmentation",
                    "running",
                    claimed.prompt_version.clone(),
                    now_ms_for_command(),
                ))?;
            let error = finish_failed(
                &database,
                &claimed,
                attempt_id,
                WorkerFailure::business(
                    "章节校验未通过，请重试。",
                    format!("business validation failure on attempt {attempt_number}"),
                ),
            )
            .expect_err("business failure must not be treated as success");
            assert_eq!(error.code, "ChapterAnalysisFailed");

            let persisted = database
                .repository()
                .get_agent_task(claimed.id)?
                .ok_or("chapter retry task disappeared")?;
            assert_eq!(persisted.status, *expected_status);
            assert_eq!(persisted.attempt_count, attempt_number);
            assert_eq!(persisted.output_json, None);

            let attempt = database
                .repository()
                .get_agent_attempt(attempt_id)?
                .ok_or("chapter attempt disappeared")?;
            assert_eq!(attempt.attempt_number, attempt_number);
            assert_eq!(attempt.status, "failed");
            assert!(attempt
                .validation_report
                .as_deref()
                .is_some_and(|report| report.contains("章节校验未通过")));
            attempt_ids.push(attempt_id);

            let chapters = database
                .repository()
                .list_chapters_by_agent_task(claimed.id, episode_id)?;
            assert_eq!(chapters.len(), 1);
            assert_eq!(chapters[0].id, chapter_id);
            assert_eq!(chapters[0].status, "draft");
        }

        assert_eq!(attempt_ids.len(), 3);
        assert!(attempt_ids.windows(2).all(|pair| pair[0] != pair[1]));
        assert!(database
            .repository()
            .claim_agent_task_by_key(task_key)?
            .is_none());

        let final_task = database
            .repository()
            .get_agent_task(task.id)?
            .ok_or("final chapter retry task disappeared")?;
        assert_eq!(final_task.status, "failed");
        assert_eq!(final_task.attempt_count, 3);
        assert_eq!(final_task.max_attempts, 3);
        assert_eq!(final_task.output_json, None);
        Ok(())
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

    fn running_test_task(
        database: &mut Database,
        task_key: &str,
    ) -> Result<(AgentTaskRecord, i64), Box<dyn std::error::Error>> {
        let repository = database.repository();
        let input = NewAgentTask::new(task_key, "chapter_segmentation", "chapter.v1");
        let created = repository.get_or_create_agent_task(&input)?;
        let task = repository
            .claim_agent_task_by_key(&created.task_key)?
            .ok_or("test task was not claimable")?;
        let attempt_id = repository.insert_agent_attempt(&NewAgentAttempt::new(
            task.id,
            task.attempt_count,
            "chapter_segmentation",
            "running",
            task.prompt_version.clone(),
            now_ms_for_command(),
        ))?;
        Ok((task, attempt_id))
    }

    #[test]
    fn non_json_assistant_final_succeeds_after_durable_finalize(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut database = Database::open_in_memory()?;
        let (task, attempt_id) = running_test_task(&mut database, "scoped-finalized")?;
        assert!(database.repository().complete_agent_task(
            task.id,
            "succeeded",
            task.attempt_count,
            0,
            None,
            r#"{"finalizedBy":"chapter_tool"}"#,
        )?);

        let result = settle_attempt(
            &mut database,
            &task,
            attempt_id,
            AttemptResult {
                assistant_response: "Done — the chapter tools finalized the task.".to_string(),
            },
        );
        assert!(result.is_ok(), "durable finalize should determine success");
        Ok(())
    }

    #[test]
    fn non_json_assistant_final_without_finalize_fails_as_scoped_completion(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut database = Database::open_in_memory()?;
        let (task, attempt_id) = running_test_task(&mut database, "scoped-unfinalized")?;

        let error = settle_attempt(
            &mut database,
            &task,
            attempt_id,
            AttemptResult {
                assistant_response: "I completed the chapters.".to_string(),
            },
        )
        .expect_err("assistant text must not complete a scoped task");
        assert_eq!(error.message, "章节 Agent 未完成章节写入，请再次尝试。");
        let persisted = database
            .repository()
            .get_agent_task(task.id)?
            .ok_or("test task disappeared")?;
        assert_eq!(persisted.status, "validation_failure");
        let report = persisted
            .validation_report
            .ok_or("scoped completion failure was not persisted")?;
        assert!(report.contains("章节 Agent 未完成章节写入，请再次尝试"));
        assert!(!report.contains("I completed the chapters"));
        Ok(())
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
