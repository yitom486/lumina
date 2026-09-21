#![allow(dead_code)]

//! User-triggered AI chapter segmentation command.
//!
//! This is intentionally a small command boundary.  The durable task
//! repository and the dedicated Chapter ACP session are added by the next
//! migration batch; this module already owns the stable request shape and
//! starts the real media-evidence worker without blocking the command caller.

use serde::{Deserialize, Serialize};
use tauri::async_runtime;

use crate::commands::chapter_worker;

/// Temporary wire-compatible representation for the spoiler boundary.
///
/// `lumina_ai::prompts::SpoilerBoundary` is not present in this checkout yet.
/// Keeping the value opaque preserves forward-compatible JSON while the
/// prompt/validation domain type lands.
pub type SpoilerBoundary = serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSegmentationRequest {
    pub media_path: String,
    pub episode_key: String,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub profiles: Option<lumina_acp::AgentProfilesHint>,
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

impl ChapterSegmentationRequest {
    pub fn task_key(&self) -> String {
        chapter_task_key(&self.media_path, &self.episode_key)
    }

    pub fn has_complete_worker_config(&self) -> bool {
        chapter_worker::has_complete_worker_config(
            self.profile_id.as_deref(),
            self.profiles.as_ref(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSegmentationStartResponse {
    pub task_key: String,
    pub queued: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ChapterErrorCode {
    InvalidInput,
    RemoteMedia,
    MediaMissing,
    ProbeFailed,
    SubtitleFailed,
    CaptureFailed,
    InternalError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterError {
    pub code: ChapterErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ChapterError {
    fn new(code: ChapterErrorCode, message: &'static str, details: Option<String>) -> Self {
        Self {
            code,
            message: message.to_owned(),
            details,
        }
    }

    pub fn invalid_input(details: Option<&str>) -> Self {
        Self::new(
            ChapterErrorCode::InvalidInput,
            "章节任务参数无效",
            details.map(str::to_owned),
        )
    }

    pub fn remote_media(details: Option<&str>) -> Self {
        Self::new(
            ChapterErrorCode::RemoteMedia,
            "在线视频暂不支持本地 AI 章节分段",
            details.map(str::to_owned),
        )
    }

    pub fn media_missing(details: Option<&str>) -> Self {
        Self::new(
            ChapterErrorCode::MediaMissing,
            "当前媒体文件不存在或无法访问",
            details.map(str::to_owned),
        )
    }

    pub fn probe_failed(details: Option<&str>) -> Self {
        Self::new(
            ChapterErrorCode::ProbeFailed,
            "无法读取该视频的媒体信息",
            details.map(str::to_owned),
        )
    }

    pub fn subtitle_failed(details: Option<&str>) -> Self {
        Self::new(
            ChapterErrorCode::SubtitleFailed,
            "无法读取字幕证据",
            details.map(str::to_owned),
        )
    }

    pub fn capture_failed(details: Option<&str>) -> Self {
        Self::new(
            ChapterErrorCode::CaptureFailed,
            "无法准备章节画面证据",
            details.map(str::to_owned),
        )
    }

    pub fn internal(details: Option<&str>) -> Self {
        Self::new(
            ChapterErrorCode::InternalError,
            "内部错误，请重试",
            details.map(str::to_owned),
        )
    }
}

/// Stable task identity.  Worker configuration is deliberately excluded.
pub fn chapter_task_key(media_path: &str, episode_key: &str) -> String {
    format!("{media_path}\u{001f}{episode_key}")
}

/// Start is intentionally fire-and-forget: media probing and evidence
/// preparation must never delay the IPC response or touch the player HWND.
#[tauri::command]
pub async fn chapter_segmentation_start(
    request: ChapterSegmentationRequest,
) -> Result<ChapterSegmentationStartResponse, ChapterError> {
    if request.media_path.trim().is_empty() || request.episode_key.trim().is_empty() {
        return Err(ChapterError::invalid_input(Some(
            "mediaPath and episodeKey are required",
        )));
    }

    let task_key = request.task_key();
    let queued = request.has_complete_worker_config();
    if queued {
        let worker_request = request.clone();
        std::mem::drop(async_runtime::spawn_blocking(move || {
            if let Err(error) = chapter_worker::run(worker_request) {
                tracing::warn!(
                    code = ?error.code,
                    message = %error.message,
                    details = ?error.details,
                    "chapter worker stopped"
                );
            }
        }));
    }

    Ok(ChapterSegmentationStartResponse { task_key, queued })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn profiles() -> lumina_acp::AgentProfilesHint {
        lumina_acp::AgentProfilesHint {
            active_profile_id: "codex".to_owned(),
            profiles: vec![lumina_acp::AgentProfileInput {
                id: "codex".to_owned(),
                name: "Codex".to_owned(),
                kind: lumina_acp::AgentKind::Codex,
                command: "codex-acp".to_owned(),
                args: Vec::new(),
                env: HashMap::new(),
                launcher: None,
                env_preset: None,
                auth_policy: None,
                auth_methods: Vec::new(),
                session_storage: None,
            }],
        }
    }

    #[test]
    fn task_key_ignores_worker_configuration() {
        let first = ChapterSegmentationRequest {
            media_path: "C:/video/a.mkv".to_owned(),
            episode_key: "S01E01".to_owned(),
            profile_id: Some("codex".to_owned()),
            profiles: Some(profiles()),
            model_id: Some("model-a".to_owned()),
            reasoning_effort: Some("low".to_owned()),
            subtitle_choice_id: Some("embedded:1".to_owned()),
            position_ms: Some(1_000),
            spoiler_boundary: Some(serde_json::json!({"positionMs": 1_000})),
        };
        let second = ChapterSegmentationRequest {
            model_id: Some("model-b".to_owned()),
            reasoning_effort: Some("high".to_owned()),
            ..first.clone()
        };
        assert_eq!(first.task_key(), second.task_key());
    }

    #[test]
    fn worker_configuration_requires_matching_runnable_profile() {
        let mut request = ChapterSegmentationRequest {
            media_path: "C:/video/a.mkv".to_owned(),
            episode_key: "S01E01".to_owned(),
            profile_id: Some("codex".to_owned()),
            profiles: Some(profiles()),
            model_id: None,
            reasoning_effort: None,
            subtitle_choice_id: None,
            position_ms: None,
            spoiler_boundary: None,
        };

        assert!(request.has_complete_worker_config());

        request.profile_id = Some("missing".to_owned());
        assert!(!request.has_complete_worker_config());

        request.profile_id = Some("codex".to_owned());
        request.profiles = None;
        assert!(!request.has_complete_worker_config());
    }
}
