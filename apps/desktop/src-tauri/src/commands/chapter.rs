//! Tauri boundary for durable, user-triggered chapter segmentation tasks.
//!
//! The command owns the durable task boundary; the worker module owns the
//! asynchronous evidence collection, isolated ACP execution and validation.

use std::path::PathBuf;

use lumina_acp::AgentProfilesHint;
use lumina_ai::prompts::{SpoilerBoundary, TaskId, ValidationReport};
use lumina_library::{AgentTaskRecord, Database, DatabaseError, NewAgentTask};
use serde::{Deserialize, Serialize};

#[path = "chapter_worker.rs"]
mod chapter_worker;

const CHAPTER_TASK_TYPE: &str = "chapter_segmentation";
const MAX_ATTEMPTS: i64 = 3;

/// Stable identity supplied by the user-triggered chapter segmentation entry
/// point.  It is intentionally independent from the active chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSegmentationRequest {
    pub media_path: String,
    pub episode_key: String,
    #[serde(default)]
    pub episode_identity: Option<ChapterEpisodeIdentity>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub profiles: Option<AgentProfilesHint>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub subtitle_choice_id: Option<String>,
    #[serde(default)]
    pub position_ms: Option<u64>,
    #[serde(default)]
    pub spoiler_boundary: Option<SpoilerBoundary>,
}

/// Identity supplied by the library-backed desktop entry point.
///
/// `Authoritative` is reserved for a matched TMDb TV episode with explicit
/// season/episode metadata. `Legacy` is intentional compatibility state: the
/// task remains path-based when the library context is absent or incomplete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChapterEpisodeIdentity {
    Authoritative {
        series_stable_id: String,
        episode_stable_id: String,
        season: u32,
        episode: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        series_title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Legacy {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

type AuthoritativeParts<'a> = (&'a str, &'a str, u32, u32, Option<&'a str>, Option<&'a str>);

impl ChapterEpisodeIdentity {
    fn validate(&self) -> Result<(), ChapterCommandError> {
        let Self::Authoritative {
            series_stable_id,
            episode_stable_id,
            season,
            episode,
            ..
        } = self
        else {
            return Ok(());
        };
        if !is_positive_tmdb_tv_stable_id(series_stable_id)
            || series_stable_id != series_stable_id.trim()
            || episode_stable_id != episode_stable_id.trim()
            || *season == 0
            || *episode == 0
            || episode_stable_id != &format!("s{season:02}e{episode:02}")
        {
            return Err(ChapterCommandError::invalid("章节身份信息不完整"));
        }
        Ok(())
    }

    fn authoritative_parts(&self) -> Option<AuthoritativeParts<'_>> {
        if self.validate().is_err() {
            return None;
        }
        match self {
            Self::Authoritative {
                series_stable_id,
                episode_stable_id,
                season,
                episode,
                series_title,
                title,
            } => Some((
                series_stable_id.trim(),
                episode_stable_id.trim(),
                *season,
                *episode,
                series_title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
            )),
            Self::Legacy { .. } => None,
        }
    }
}

fn is_positive_tmdb_tv_stable_id(value: &str) -> bool {
    let Some(tmdb_id) = value.strip_prefix("tmdb:tv:") else {
        return false;
    };
    !tmdb_id.is_empty()
        && tmdb_id.bytes().all(|byte| byte.is_ascii_digit())
        && tmdb_id.parse::<u64>().map(|id| id > 0).unwrap_or(false)
}

/// Safe failure information persisted inside the existing task report column.
/// The nested validation report remains a Rust-side artifact; the UI receives
/// only the derived business summary below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PersistedChapterFailure {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_report: Option<ValidationReport>,
}

/// Durable business snapshot returned to the UI. Native diagnostics and raw
/// validation reports stay in the task record/logs and are never serialized
/// through this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSegmentationSnapshot {
    pub id: i64,
    pub task_key: String,
    pub task_type: String,
    pub episode_id: Option<i64>,
    pub chapter_id: Option<i64>,
    pub episode_identity: ChapterEpisodeIdentity,
    pub status: String,
    pub session_id: Option<String>,
    pub prompt_version: String,
    pub output_contract_version: Option<String>,
    pub attempt_count: i64,
    pub retry_count: i64,
    pub max_attempts: i64,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
    pub validation_summary: Option<String>,
    pub can_retry: bool,
    pub retry_action: Option<String>,
    pub agent_configured: bool,
    pub output_json: Option<String>,
    pub draft_chapters: Vec<ChapterDraftSnapshot>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Small, durable chapter-task projection for the chapters panel.  It is
/// intentionally independent from the legacy task output JSON so an outline
/// can be rendered before the Agent session has finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterDraftSnapshot {
    pub id: i64,
    pub stable_id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: Option<String>,
    pub mainline: Option<String>,
    pub status: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FailureProjection {
    code: String,
    message: String,
    summary: Option<String>,
}

/// Stable business error shape for both chapter commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterCommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ChapterCommandError {
    fn invalid(message: &'static str) -> Self {
        Self {
            code: "InvalidInput".to_string(),
            message: message.to_string(),
            details: None,
        }
    }

    fn not_found() -> Self {
        Self {
            code: "NotFound".to_string(),
            message: "尚未开始该媒体的 AI 分段".to_string(),
            details: None,
        }
    }

    fn storage(error: DatabaseError) -> Self {
        tracing::warn!(
            code = ?error.code,
            details = ?error.details,
            "chapter segmentation storage operation failed"
        );
        Self {
            code: "StorageError".to_string(),
            message: "章节分段任务暂时不可用，请重试".to_string(),
            details: error.details,
        }
    }

    fn internal(details: impl Into<String>) -> Self {
        let details = details.into();
        tracing::error!(details = %details, "chapter segmentation command failed");
        Self {
            code: "InternalError".to_string(),
            message: "内部错误，请重试".to_string(),
            details: Some(details),
        }
    }
}

#[tauri::command]
pub async fn chapter_segmentation_start(
    request: ChapterSegmentationRequest,
) -> Result<ChapterSegmentationSnapshot, ChapterCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let worker_request = request.clone();
        let snapshot = create_or_load_task(request)?;
        if worker_request.worker_ready()
            && matches!(snapshot.status.as_str(), "pending" | "validation_failure")
        {
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = chapter_worker::run(worker_request) {
                    tracing::warn!(code = %error.code, details = ?error.details, "chapter worker stopped");
                }
            });
        }
        Ok(snapshot)
    })
        .await
        .map_err(|error| ChapterCommandError::internal(format!("chapter task join: {error}")))?
}

#[tauri::command]
pub async fn chapter_segmentation_status(
    request: ChapterSegmentationRequest,
) -> Result<ChapterSegmentationSnapshot, ChapterCommandError> {
    tauri::async_runtime::spawn_blocking(move || load_task(request))
        .await
        .map_err(|error| ChapterCommandError::internal(format!("chapter status join: {error}")))?
}

fn create_or_load_task(
    request: ChapterSegmentationRequest,
) -> Result<ChapterSegmentationSnapshot, ChapterCommandError> {
    validate_request(&request)?;
    let definition = TaskId::ChapterSegment.definition();
    let mut input = NewAgentTask::new(
        task_key(&request),
        CHAPTER_TASK_TYPE,
        definition.version.to_string(),
    );
    input.output_contract_version = Some(definition.output_contract_version.to_string());
    input.max_attempts = MAX_ATTEMPTS;

    let database = open_database()?;
    let task = database
        .repository()
        .get_or_create_agent_task(&input)
        .map_err(ChapterCommandError::storage)?;
    let task = normalize_retryable_failed_task(&database, task)?;
    let drafts = draft_chapters_for_task(&database, &task)?;
    Ok(snapshot_from_task(
        task,
        request.worker_ready(),
        request.episode_identity.as_ref(),
        drafts,
    ))
}

fn load_task(
    request: ChapterSegmentationRequest,
) -> Result<ChapterSegmentationSnapshot, ChapterCommandError> {
    validate_request(&request)?;
    let task_key = task_key(&request);
    let database = open_database()?;
    let task = database
        .repository()
        .get_agent_task_by_key(&task_key)
        .map_err(ChapterCommandError::storage)?;
    task.map(|task| {
        let drafts = draft_chapters_for_task(&database, &task)?;
        Ok(snapshot_from_task(
            task,
            request.worker_ready(),
            request.episode_identity.as_ref(),
            drafts,
        ))
    })
    .transpose()?
    .ok_or_else(ChapterCommandError::not_found)
}

fn draft_chapters_for_task(
    database: &Database,
    task: &AgentTaskRecord,
) -> Result<Vec<ChapterDraftSnapshot>, ChapterCommandError> {
    let Some(episode_id) = task.episode_id else {
        return Ok(Vec::new());
    };
    database
        .repository()
        .list_chapters_by_agent_task(task.id, episode_id)
        .map(|chapters| {
            chapters
                .into_iter()
                .map(|chapter| ChapterDraftSnapshot {
                    id: chapter.id,
                    stable_id: chapter.stable_id,
                    start_ms: chapter.start_ms,
                    end_ms: chapter.end_ms,
                    title: chapter.title,
                    mainline: chapter.mainline,
                    status: project_chapter_status(&chapter.status, &task.status).to_string(),
                    updated_at_ms: chapter.updated_at_ms,
                })
                .collect()
        })
        .map_err(ChapterCommandError::storage)
}

fn project_chapter_status(chapter_status: &str, task_status: &str) -> &'static str {
    match chapter_status {
        "waiting_evidence" | "awaiting_evidence" | "draft" => "waiting_evidence",
        "analyzing" | "analysis" | "in_progress" => "analyzing",
        "generated" | "ready" | "published" | "accepted" => "generated",
        "validation_failed" | "validation_failure" => "validation_failed",
        _ if matches!(task_status, "validation_failure" | "failed") => "validation_failed",
        _ => "analyzing",
    }
}

fn normalize_retryable_failed_task(
    database: &Database,
    task: AgentTaskRecord,
) -> Result<AgentTaskRecord, ChapterCommandError> {
    if task.status != "failed" || task.attempt_count >= task.max_attempts {
        return Ok(task);
    }
    let changed = database
        .repository()
        .update_agent_task_status(
            task.id,
            "validation_failure",
            task.attempt_count,
            task.retry_count,
            task.validation_report.as_deref(),
        )
        .map_err(ChapterCommandError::storage)?;
    if !changed {
        return Err(ChapterCommandError::internal(
            "retryable chapter task disappeared",
        ));
    }
    database
        .repository()
        .get_agent_task(task.id)
        .map_err(ChapterCommandError::storage)?
        .ok_or_else(|| {
            ChapterCommandError::internal("retryable chapter task could not be reloaded")
        })
}

fn validate_request(request: &ChapterSegmentationRequest) -> Result<(), ChapterCommandError> {
    if request.media_path.trim().is_empty() {
        return Err(ChapterCommandError::invalid("媒体路径不能为空"));
    }
    if request.episode_key.trim().is_empty() {
        return Err(ChapterCommandError::invalid("集数标识不能为空"));
    }
    if let Some(identity) = request.episode_identity.as_ref() {
        identity.validate()?;
    }
    Ok(())
}

fn task_key(request: &ChapterSegmentationRequest) -> String {
    if let Some((series_stable_id, episode_stable_id, ..)) = request
        .episode_identity
        .as_ref()
        .and_then(ChapterEpisodeIdentity::authoritative_parts)
    {
        return format!("chapter-segmentation:identity:{series_stable_id}:{episode_stable_id}");
    }
    format!(
        "chapter-segmentation:{}:{}",
        request.media_path.trim(),
        request.episode_key.trim()
    )
}

pub(super) fn database_path() -> Result<PathBuf, ChapterCommandError> {
    let base = super::system::data_dir()
        .ok_or_else(|| ChapterCommandError::internal("application data directory unavailable"))?;
    Ok(base.join("lumina").join("lumina.sqlite3"))
}

pub(super) fn open_database() -> Result<Database, ChapterCommandError> {
    let path = database_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            ChapterCommandError::internal(format!("create chapter data directory: {error}"))
        })?;
    }
    Database::open(path).map_err(ChapterCommandError::storage)
}

pub(super) fn now_ms_for_command() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

/// Reconcile chapter tasks left in `running` by an earlier process before the
/// UI can inspect or retry them. This is called once during application setup,
/// never from a status query while a worker may still be active.
pub(crate) fn recover_interrupted_tasks() {
    match open_database().and_then(|mut database| {
        database
            .recover_interrupted_agent_tasks()
            .map_err(ChapterCommandError::storage)
    }) {
        Ok(count) if count > 0 => {
            tracing::info!(count, "recovered interrupted chapter tasks");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(code = %error.code, details = ?error.details, "chapter task recovery skipped");
        }
    }
}

impl ChapterSegmentationRequest {
    fn worker_ready(&self) -> bool {
        let Some(profile_id) = self
            .profile_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        else {
            return false;
        };
        self.profiles.as_ref().is_some_and(|profiles| {
            profiles
                .profiles
                .iter()
                .any(|profile| profile.id == profile_id)
        })
    }
}

fn snapshot_from_task(
    task: AgentTaskRecord,
    agent_configured: bool,
    episode_identity: Option<&ChapterEpisodeIdentity>,
    draft_chapters: Vec<ChapterDraftSnapshot>,
) -> ChapterSegmentationSnapshot {
    let failure = if matches!(task.status.as_str(), "validation_failure" | "failed") {
        task.validation_report
            .as_deref()
            .and_then(project_failure)
            .or_else(|| {
                Some(FailureProjection {
                    code: "ChapterAnalysisFailed".to_string(),
                    message: stable_failure_message("ChapterAnalysisFailed", None).to_string(),
                    summary: None,
                })
            })
    } else {
        None
    };
    let requires_agent_configuration = !agent_configured
        || failure
            .as_ref()
            .is_some_and(|value| value.code == "AgentNotConfigured");
    let can_retry = matches!(task.status.as_str(), "validation_failure" | "failed")
        && task.attempt_count < task.max_attempts
        && !requires_agent_configuration;
    let retry_action = if requires_agent_configuration
        && matches!(
            task.status.as_str(),
            "pending" | "validation_failure" | "failed"
        ) {
        Some("configure_agent".to_string())
    } else if can_retry {
        Some("retry".to_string())
    } else {
        None
    };
    ChapterSegmentationSnapshot {
        id: task.id,
        task_key: task.task_key,
        task_type: task.task_type,
        episode_id: task.episode_id,
        chapter_id: task.chapter_id,
        episode_identity: episode_identity
            .cloned()
            .unwrap_or(ChapterEpisodeIdentity::Legacy {
                reason: Some("identity_not_provided".to_string()),
            }),
        status: task.status,
        session_id: task.session_id,
        prompt_version: task.prompt_version,
        output_contract_version: task.output_contract_version,
        attempt_count: task.attempt_count,
        retry_count: task.retry_count,
        max_attempts: task.max_attempts,
        failure_code: failure.as_ref().map(|value| value.code.clone()),
        failure_message: failure.as_ref().map(|value| value.message.clone()),
        validation_summary: failure.and_then(|value| value.summary),
        can_retry,
        retry_action,
        agent_configured,
        output_json: task.output_json,
        draft_chapters,
        created_at_ms: task.created_at_ms,
        updated_at_ms: task.updated_at_ms,
    }
}

fn project_failure(value: &str) -> Option<FailureProjection> {
    if let Ok(stored) = serde_json::from_str::<PersistedChapterFailure>(value) {
        let summary = stored.summary.or_else(|| {
            stored
                .validation_report
                .as_ref()
                .and_then(validation_summary)
        });
        return Some(FailureProjection {
            code: stored.code.clone(),
            message: stable_failure_message(&stored.code, Some(&stored.message)).to_string(),
            summary,
        });
    }
    if let Ok(report) = serde_json::from_str::<ValidationReport>(value) {
        return Some(FailureProjection {
            code: "ValidationFailed".to_string(),
            message: stable_failure_message("ValidationFailed", None).to_string(),
            summary: validation_summary(&report),
        });
    }
    if value == "应用关闭时章节任务中断，可重新执行" {
        return Some(FailureProjection {
            code: "Interrupted".to_string(),
            message: stable_failure_message("Interrupted", None).to_string(),
            summary: Some("上一次分析在应用关闭时中断，可以再次尝试。".to_string()),
        });
    }
    None
}

fn stable_failure_message(code: &str, persisted_message: Option<&str>) -> &'static str {
    match code {
        "AgentNotConfigured" => "尚未配置可用的 AI Agent，请先完成 Agent 设置。",
        "AgentUnavailable" => "章节 Agent 暂时不可用，请检查 Agent 设置后重试。",
        "MediaUnavailable" => "媒体文件不可用，请重新打开后重试。",
        "EvidenceUnavailable" => "未找到可用于章节分析的字幕或画面证据，请检查媒体资源后重试。",
        "ValidationFailed" => "章节分段结果未通过校验。",
        "Interrupted" => "上一次章节分析被中断，可以再次尝试。",
        // Older persisted worker failures used the broad ChapterAnalysisFailed
        // code. Keep their known business messages readable, but never trust an
        // arbitrary persisted string because it may contain stderr, paths, SQL,
        // or JSON-RPC details.
        "ChapterAnalysisFailed" => match persisted_message {
            Some("章节分段结果保存失败，请重试") => "章节分段结果保存失败，请重试",
            Some("章节结果写入失败，请重试") => "章节结果写入失败，请重试",
            Some("无法读取媒体信息，请检查媒体文件") => {
                "无法读取媒体信息，请检查媒体文件"
            }
            Some("无法读取媒体时长，暂时不能生成章节") => {
                "无法读取媒体时长，暂时不能生成章节"
            }
            Some("未找到可用于章节分析的字幕或画面证据") => {
                "未找到可用于章节分析的字幕或画面证据"
            }
            Some("章节分析提示词准备失败，请重试") => {
                "章节分析提示词准备失败，请重试"
            }
            Some("章节 Agent 工作区不可用，请重试") => {
                "章节 Agent 工作区不可用，请重试"
            }
            Some("当前媒体不支持 AI 分段") => "当前媒体不支持 AI 分段",
            Some("媒体文件不可用，请重新打开后重试") => {
                "媒体文件不可用，请重新打开后重试"
            }
            Some("字幕证据准备失败，请重试") => "字幕证据准备失败，请重试",
            Some("当前字幕格式无法用于章节分析") => "当前字幕格式无法用于章节分析",
            Some("画面证据准备失败，请重试") => "画面证据准备失败，请重试",
            _ => "章节分段未完成，请再次尝试。",
        },
        _ => "章节分段未完成，请再次尝试。",
    }
}

fn validation_summary(report: &ValidationReport) -> Option<String> {
    let issue = report.issues.first()?;
    let summary = match issue.error_code.as_str() {
        "invalid_structured_output" => "AI 返回的章节结果格式不完整。",
        "missing_chapters" => "AI 没有返回可用的章节内容。",
        "missing_field" | "empty_chapter_title" | "empty_chapter_mainline" => {
            "章节结果缺少必要内容。"
        }
        "invalid_chapter_interval"
        | "chapter_out_of_media_bounds"
        | "chapter_overlap"
        | "chapters_not_ordered" => "章节时间范围或顺序不符合视频时间轴。",
        "invalid_evidence_reference"
        | "missing_evidence"
        | "unknown_screenshot_reference"
        | "unknown_transcript_reference"
        | "evidence_outside_chapter" => "章节引用的字幕或画面证据无效。",
        _ => "章节结果未通过结构和时间轴校验。",
    };
    let remaining = report.issues.len().saturating_sub(1);
    Some(if remaining == 0 {
        summary.to_string()
    } else {
        format!("{summary} 另有 {remaining} 项需要修正。")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_key_is_stable_for_media_and_episode() {
        let request = ChapterSegmentationRequest {
            media_path: r"C:\media\episode-1.mkv".to_string(),
            episode_key: "s01e07".to_string(),
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
            task_key(&request),
            r"chapter-segmentation:C:\media\episode-1.mkv:s01e07"
        );
        assert_eq!(task_key(&request), task_key(&request));
    }

    #[test]
    fn request_and_error_use_camel_case_and_business_message() {
        let request = ChapterSegmentationRequest {
            media_path: "media-1.mkv".to_string(),
            episode_key: "episode-2".to_string(),
            episode_identity: None,
            profile_id: None,
            profiles: None,
            model_id: None,
            reasoning_effort: None,
            subtitle_choice_id: None,
            position_ms: None,
            spoiler_boundary: None,
        };
        let request_json = match serde_json::to_value(request) {
            Ok(value) => value,
            Err(error) => panic!("request should serialize: {error}"),
        };
        assert_eq!(
            request_json.get("mediaPath").and_then(|v| v.as_str()),
            Some("media-1.mkv")
        );
        assert_eq!(
            request_json.get("episodeKey").and_then(|v| v.as_str()),
            Some("episode-2")
        );

        let error = ChapterCommandError::storage(DatabaseError {
            code: lumina_library::DatabaseErrorCode::OpenFailed,
            message: "无法打开应用数据存储".to_string(),
            details: Some("open C:\\private\\library.sqlite3: sqlite detail".to_string()),
        });
        assert!(error.message.contains('章'));
        assert!(!error.message.contains("sqlite"));
        let error_json = match serde_json::to_value(error) {
            Ok(value) => value,
            Err(error) => panic!("error should serialize: {error}"),
        };
        assert_eq!(
            error_json.get("code").and_then(|v| v.as_str()),
            Some("StorageError")
        );
        assert!(error_json.get("details").is_some());
    }

    fn task_with_failure(status: &str, attempt_count: i64, max_attempts: i64) -> AgentTaskRecord {
        let report = PersistedChapterFailure {
            code: "ValidationFailed".to_string(),
            message: "raw details must not be exposed".to_string(),
            summary: Some("章节结果缺少必要内容。".to_string()),
            validation_report: None,
        };
        AgentTaskRecord {
            id: 7,
            task_key: "chapter-segmentation:test".to_string(),
            task_type: CHAPTER_TASK_TYPE.to_string(),
            episode_id: None,
            chapter_id: None,
            status: status.to_string(),
            session_id: None,
            prompt_version: "1.0".to_string(),
            output_contract_version: Some("chapter_tool_workflow.v1".to_string()),
            attempt_count,
            retry_count: 3,
            max_attempts,
            validation_report: Some(serde_json::to_string(&report).expect("test report")),
            output_json: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn snapshot_serializes_safe_failure_projection_and_retry_budget() {
        let snapshot = snapshot_from_task(
            task_with_failure("validation_failure", 1, 3),
            true,
            None,
            vec![],
        );
        let value = serde_json::to_value(snapshot).expect("snapshot should serialize");

        assert_eq!(
            value.get("failureCode").and_then(|v| v.as_str()),
            Some("ValidationFailed")
        );
        assert_eq!(
            value.get("failureMessage").and_then(|v| v.as_str()),
            Some("章节分段结果未通过校验。")
        );
        assert_eq!(
            value.get("validationSummary").and_then(|v| v.as_str()),
            Some("章节结果缺少必要内容。")
        );
        assert_eq!(value.get("attemptCount").and_then(|v| v.as_i64()), Some(1));
        assert_eq!(value.get("maxAttempts").and_then(|v| v.as_i64()), Some(3));
        assert_eq!(value.get("canRetry").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            value.get("retryAction").and_then(|v| v.as_str()),
            Some("retry")
        );
        assert!(value.get("validationReport").is_none());
        assert!(!value.to_string().contains("raw details"));

        let terminal = snapshot_from_task(task_with_failure("failed", 3, 3), true, None, vec![]);
        assert!(!terminal.can_retry);
        assert_eq!(terminal.retry_action, None);
    }

    #[test]
    fn persisted_failure_message_uses_business_whitelist_without_leaking_details() {
        let legacy_messages = [
            ("字幕证据准备失败，请重试", "字幕证据准备失败，请重试"),
            ("画面证据准备失败，请重试", "画面证据准备失败，请重试"),
            (
                "媒体文件不可用，请重新打开后重试",
                "媒体文件不可用，请重新打开后重试",
            ),
            ("章节结果写入失败，请重试", "章节结果写入失败，请重试"),
        ];

        for (persisted_message, expected_message) in legacy_messages {
            let persisted = PersistedChapterFailure {
                code: "ChapterAnalysisFailed".to_string(),
                message: persisted_message.to_string(),
                summary: None,
                validation_report: None,
            };
            let failure = project_failure(&serde_json::to_string(&persisted).expect("persisted"))
                .expect("persisted failure should project");
            assert_eq!(failure.message, expected_message);
        }

        let direct_code_cases = [
            (
                "AgentNotConfigured",
                "尚未配置可用的 AI Agent，请先完成 Agent 设置。",
            ),
            (
                "AgentUnavailable",
                "章节 Agent 暂时不可用，请检查 Agent 设置后重试。",
            ),
            ("ValidationFailed", "章节分段结果未通过校验。"),
        ];
        for (code, expected_message) in direct_code_cases {
            let persisted = PersistedChapterFailure {
                code: code.to_string(),
                message: "private stderr C:\\secret\\agent.log sqlite JSON-RPC".to_string(),
                summary: None,
                validation_report: None,
            };
            let failure = project_failure(&serde_json::to_string(&persisted).expect("persisted"))
                .expect("persisted failure should project");
            assert_eq!(failure.message, expected_message);
            assert!(!failure.message.contains("private"));
        }

        let unsafe_persisted = PersistedChapterFailure {
            code: "ChapterAnalysisFailed".to_string(),
            message: "stderr C:\\secret\\worker.log; SQL: SELECT * FROM tasks; JSON-RPC"
                .to_string(),
            summary: None,
            validation_report: None,
        };
        let unsafe_failure =
            project_failure(&serde_json::to_string(&unsafe_persisted).expect("persisted"))
                .expect("persisted failure should project");
        assert_eq!(unsafe_failure.message, "章节分段未完成，请再次尝试。");
        assert!(!unsafe_failure.message.contains("stderr"));
        assert!(!unsafe_failure.message.contains("secret"));
        assert!(!unsafe_failure.message.contains("SQL"));
        assert!(!unsafe_failure.message.contains("JSON-RPC"));
    }

    #[test]
    fn authoritative_identity_makes_task_key_path_independent() {
        let identity = ChapterEpisodeIdentity::Authoritative {
            series_stable_id: "tmdb:tv:123".to_string(),
            episode_stable_id: "s02e07".to_string(),
            season: 2,
            episode: 7,
            series_title: Some("示例剧集".to_string()),
            title: Some("第七集".to_string()),
        };
        assert!(identity.validate().is_ok());
        let first = ChapterSegmentationRequest {
            media_path: r"D:\library\old\episode.mkv".to_string(),
            episode_key: r"D:\library\old\episode.mkv".to_string(),
            episode_identity: Some(identity.clone()),
            profile_id: None,
            profiles: None,
            model_id: None,
            reasoning_effort: None,
            subtitle_choice_id: None,
            position_ms: None,
            spoiler_boundary: None,
        };
        let second = ChapterSegmentationRequest {
            media_path: r"E:\library\new\renamed.mkv".to_string(),
            episode_key: r"E:\library\new\renamed.mkv".to_string(),
            episode_identity: Some(identity),
            profile_id: None,
            profiles: None,
            model_id: None,
            reasoning_effort: None,
            subtitle_choice_id: None,
            position_ms: None,
            spoiler_boundary: None,
        };
        assert_eq!(task_key(&first), task_key(&second));
        assert_eq!(
            task_key(&first),
            "chapter-segmentation:identity:tmdb:tv:123:s02e07"
        );
    }

    #[test]
    fn authoritative_identity_requires_tmdb_tv_shape_and_consistent_episode_key() {
        let invalid_identities = [
            ChapterEpisodeIdentity::Authoritative {
                series_stable_id: "123".to_string(),
                episode_stable_id: "s02e07".to_string(),
                season: 2,
                episode: 7,
                series_title: None,
                title: None,
            },
            ChapterEpisodeIdentity::Authoritative {
                series_stable_id: "tmdb:movie:123".to_string(),
                episode_stable_id: "s02e07".to_string(),
                season: 2,
                episode: 7,
                series_title: None,
                title: None,
            },
            ChapterEpisodeIdentity::Authoritative {
                series_stable_id: "tmdb:tv:0".to_string(),
                episode_stable_id: "s02e07".to_string(),
                season: 2,
                episode: 7,
                series_title: None,
                title: None,
            },
            ChapterEpisodeIdentity::Authoritative {
                series_stable_id: "tmdb:tv:abc".to_string(),
                episode_stable_id: "s02e07".to_string(),
                season: 2,
                episode: 7,
                series_title: None,
                title: None,
            },
            ChapterEpisodeIdentity::Authoritative {
                series_stable_id: "tmdb:tv:123 ".to_string(),
                episode_stable_id: "s02e07".to_string(),
                season: 2,
                episode: 7,
                series_title: None,
                title: None,
            },
            ChapterEpisodeIdentity::Authoritative {
                series_stable_id: "tmdb:tv:123".to_string(),
                episode_stable_id: "s02e08".to_string(),
                season: 2,
                episode: 7,
                series_title: None,
                title: None,
            },
            ChapterEpisodeIdentity::Authoritative {
                series_stable_id: "tmdb:tv:123".to_string(),
                episode_stable_id: "s00e07".to_string(),
                season: 0,
                episode: 7,
                series_title: None,
                title: None,
            },
        ];

        for identity in invalid_identities {
            assert!(identity.validate().is_err());
            assert!(identity.authoritative_parts().is_none());
        }
    }

    #[test]
    fn legacy_identity_remains_valid_and_never_produces_authoritative_parts() {
        let identity = ChapterEpisodeIdentity::Legacy {
            reason: Some("metadata_incomplete".to_string()),
        };

        assert!(identity.validate().is_ok());
        assert!(identity.authoritative_parts().is_none());
    }

    #[test]
    fn legacy_identity_keeps_path_compatibility_and_serializes_as_legacy() {
        let request = ChapterSegmentationRequest {
            media_path: r"C:\media\episode.mkv".to_string(),
            episode_key: r"C:\media\episode.mkv".to_string(),
            episode_identity: Some(ChapterEpisodeIdentity::Legacy {
                reason: Some("metadata_incomplete".to_string()),
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
            task_key(&request),
            r"chapter-segmentation:C:\media\episode.mkv:C:\media\episode.mkv"
        );
        let value = serde_json::to_value(request.episode_identity.expect("identity"))
            .expect("identity should serialize");
        assert_eq!(
            value.get("kind").and_then(|value| value.as_str()),
            Some("legacy")
        );
    }
}
