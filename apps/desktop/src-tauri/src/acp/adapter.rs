//! App-side ACP adapters (M5).
//!
//! Warm chat-snapshot state and all MCP/library IO live here so `lumina-acp`
//! stays a generic Client. Spawn paths (`session/new|resume`) additionally go
//! through [`AppSessionEnvironment`], installed once at startup.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use lumina_acp::{AcpError, SessionEnvironment, SessionKind, VideoPromptContext};
use lumina_core::{AgentConversation, AgentTaskError, IsolatedAgentTask};

use crate::library::MediaLibraryService;
use crate::mcp::{
    read_snapshot, snapshot_path_for_cwd, sync_snapshot_capabilities,
    write_chapter_task_snapshot as write_scoped_snapshot, write_snapshot, AgentCapabilities,
    ChapterTaskContext, LuminaMcpSnapshot, PromptAnchor, PromptSnapshotState,
};

/// MCP-backed [`SessionEnvironment`] for ACP spawn paths.
pub struct AppSessionEnvironment;

static MCP_DIAGNOSTIC_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn next_mcp_diagnostic_id() -> String {
    let sequence = MCP_DIAGNOSTIC_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{}-{unix_ms}-{sequence}", std::process::id())
}

impl SessionEnvironment for AppSessionEnvironment {
    fn snapshot_path(&self, cwd: &Path, kind: SessionKind) -> PathBuf {
        match kind {
            SessionKind::Chapter => cwd.join(".lumina").join("chapter-agent-context.json"),
            SessionKind::Chat | SessionKind::Workshop => snapshot_path_for_cwd(cwd),
        }
    }

    fn sync_snapshot(&self, snapshot_path: &Path, vision_capable: bool) -> Result<(), String> {
        // MCP 反向控制（seek）按会话工作区登记：snapshot 在
        // `<workspace>/.lumina/agent-context.json`，工作区是其祖父目录。
        if let Some(workspace) = snapshot_path.parent().and_then(|parent| parent.parent()) {
            crate::acp::mcp_control::register_workspace(workspace);
        }
        sync_snapshot_capabilities(snapshot_path, vision_capable)
    }

    fn mcp_servers(&self, snapshot_path: &Path, kind: SessionKind) -> serde_json::Value {
        let profile = mcp_profile_for_session(kind);
        let diagnostic_id = next_mcp_diagnostic_id();
        tracing::info!(
            diagnostic_id = %diagnostic_id,
            profile = %profile.env_value(),
            session_kind = ?kind,
            snapshot = %snapshot_path.display(),
            "prepared Lumina MCP server environment"
        );
        let mut servers = crate::mcp::lumina_mcp_servers(snapshot_path);
        if let Some(env) = servers
            .as_array_mut()
            .and_then(|list| list.first_mut())
            .and_then(|server| server.get_mut("env"))
            .and_then(serde_json::Value::as_array_mut)
        {
            env.push(serde_json::json!({
                "name": crate::mcp::TOOL_PROFILE_ENV,
                "value": profile.env_value(),
            }));
            env.push(serde_json::json!({
                "name": crate::mcp::DIAGNOSTIC_LOG_ENV,
                "value": crate::commands::system::log_dir()
                    .join("lumina-mcp.log")
                    .to_string_lossy(),
            }));
            env.push(serde_json::json!({
                "name": crate::mcp::DIAGNOSTIC_ID_ENV,
                "value": diagnostic_id,
            }));
        }
        servers
    }

    fn snapshot_vision_capable(&self, workspace: &Path) -> Option<bool> {
        read_snapshot(&snapshot_path_for_cwd(workspace))
            .ok()
            .and_then(|snapshot| {
                snapshot
                    .capabilities
                    .map(|capabilities| capabilities.vision_capable)
            })
    }
}

fn mcp_profile_for_session(kind: SessionKind) -> crate::mcp::McpToolProfile {
    match kind {
        SessionKind::Workshop => crate::mcp::McpToolProfile::NoTools,
        SessionKind::Chat | SessionKind::Chapter => crate::mcp::McpToolProfile::Chat,
    }
}

/// Seed the isolated chapter session with the same media anchor that the
/// interactive chat session exposes to Lumina MCP. The session remains
/// independent from chat history, but its tools must resolve the current
/// media, subtitle choice and playback boundary.
pub fn write_chapter_snapshot(
    cwd: &Path,
    media_path: &Path,
    position_ms: u64,
    duration_ms: u64,
    subtitle_choice_id: Option<&str>,
) -> Result<PathBuf, AcpError> {
    let environment = AppSessionEnvironment;
    let snapshot_path = environment.snapshot_path(cwd, SessionKind::Chapter);
    let snapshot = LuminaMcpSnapshot {
        schema_version: crate::mcp::SNAPSHOT_SCHEMA_VERSION,
        anchor: Some(PromptAnchor {
            media_path: media_path.to_string_lossy().into_owned(),
            media_title: None,
            library_root: None,
            group_key: None,
            season: None,
            episode: None,
            position_ms,
            duration_ms: Some(duration_ms),
            sent_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default(),
            subtitle_choice_id: subtitle_choice_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            transcript_window_radius_sec: None,
        }),
        current_episode: None,
        library: None,
        session: None,
        capabilities: Some(AgentCapabilities {
            vision_capable: true,
            subtitle_workshop_enabled: false,
            video_annotations_enabled: true,
        }),
        online: None,
        updated_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default(),
    };
    write_snapshot(&snapshot_path, &snapshot).map_err(|error| AcpError::internal(Some(&error)))?;
    Ok(snapshot_path)
}

/// Seed an isolated chapter session with its complete durable scope.
///
/// The MCP server must treat the snapshot as the authority for task/attempt/
/// episode ownership.  In particular, the database path is supplied by the
/// desktop command rather than by Agent arguments.
pub fn write_chapter_task_snapshot(
    cwd: &Path,
    media_path: &Path,
    position_ms: u64,
    duration_ms: u64,
    subtitle_choice_id: Option<&str>,
    chapter_task: ChapterTaskContext,
) -> Result<PathBuf, AcpError> {
    let environment = AppSessionEnvironment;
    let snapshot_path = environment.snapshot_path(cwd, SessionKind::Chapter);
    let snapshot = LuminaMcpSnapshot {
        schema_version: crate::mcp::SNAPSHOT_SCHEMA_VERSION,
        anchor: Some(PromptAnchor {
            media_path: media_path.to_string_lossy().into_owned(),
            media_title: None,
            library_root: None,
            group_key: None,
            season: None,
            episode: None,
            position_ms,
            duration_ms: Some(duration_ms),
            sent_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default(),
            subtitle_choice_id: subtitle_choice_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            transcript_window_radius_sec: None,
        }),
        current_episode: None,
        library: None,
        session: None,
        capabilities: Some(AgentCapabilities {
            vision_capable: true,
            subtitle_workshop_enabled: false,
            video_annotations_enabled: true,
        }),
        online: None,
        updated_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default(),
    };
    write_scoped_snapshot(&snapshot_path, &snapshot, &chapter_task)
        .map_err(|error| AcpError::internal(Some(&error)))?;
    Ok(snapshot_path)
}

/// Installed once at startup; `session/new|resume` wiring uses it.
pub fn install_session_environment() -> bool {
    lumina_acp::set_default_environment(Arc::new(AppSessionEnvironment))
}

/// Build the next chat prompt snapshot (warm library policy). Mirrors the
/// removed `AcpService::build_prompt_snapshot`; errors unchanged.
/// Converts the ACP prompt context into MCP's own generic input (M7).
pub fn build_prompt_snapshot(
    snapshots: &Mutex<PromptSnapshotState>,
    library: &MediaLibraryService,
    context: Option<&VideoPromptContext>,
    vision_capable: bool,
) -> Result<(LuminaMcpSnapshot, bool), AcpError> {
    let context = context.map(|context| crate::mcp::McpPromptContext {
        media_path: context.media_path.clone(),
        media_title: context.media_title.clone(),
        position_ms: context.position_ms,
        duration_ms: context.duration_ms,
        subtitle_choice_id: context.subtitle_choice_id.clone(),
        transcript_window_radius_sec: context.transcript_window_radius_sec,
    });
    let mut guard = snapshots
        .lock()
        .map_err(|_| AcpError::internal(Some("prompt snapshot mutex poisoned")))?;
    let built = guard
        .next_snapshot(context.as_ref(), library, vision_capable)
        .map_err(|error| AcpError::internal(error.details.as_deref()))?;
    Ok((built.snapshot, built.media_changed))
}

/// Persist the chat prompt snapshot. Mirrors `write_prompt_snapshot`.
pub fn write_prompt_snapshot(
    cwd: &Path,
    snapshot: &LuminaMcpSnapshot,
) -> Result<PathBuf, AcpError> {
    let path = snapshot_path_for_cwd(cwd);
    write_snapshot(&path, snapshot).map_err(|details| AcpError::internal(Some(&details)))?;
    Ok(path)
}

/// Record session capabilities. Mirrors `sync_mcp_capabilities`.
pub fn sync_mcp_capabilities(cwd_hint: Option<&str>, vision_capable: bool) -> Result<(), AcpError> {
    let workspace = lumina_acp::agent::workspace::resolve_session_cwd(cwd_hint)?;
    let path = snapshot_path_for_cwd(&workspace);
    sync_snapshot_capabilities(&path, vision_capable)
        .map_err(|details| AcpError::internal(Some(&details)))
}

/// Clear chat snapshot warm state (close / new chat). Failure is ignored,
/// mirroring the removed `reset_prompt_snapshot_state`.
pub fn reset_prompt_snapshot_state(snapshots: &Mutex<PromptSnapshotState>) {
    if let Ok(mut guard) = snapshots.lock() {
        guard.reset();
    }
}

/// ACP-backed [`lumina_core::AgentInvoker`] for subtitle workshop tasks:
/// short-lived isolated calls, no chat history, no MCP tools.
///
/// With a pool attached (workshop jobs), conversations route to a slot-pinned
/// [`lumina_acp::WorkshopPool`] lease. Without one (library resolver), each
/// call runs the legacy one-shot isolated path. The adapter itself owns no
/// pool lifecycle:
/// creation and shutdown live with the caller (commands layer).
pub struct AcpAgentInvoker {
    profiles: lumina_acp::AgentProfilesHint,
    pool: Option<Arc<lumina_acp::WorkshopPool>>,
}

impl AcpAgentInvoker {
    pub fn new(profiles: lumina_acp::AgentProfilesHint) -> Self {
        Self {
            profiles,
            pool: None,
        }
    }

    pub fn with_pool(
        profiles: lumina_acp::AgentProfilesHint,
        pool: Arc<lumina_acp::WorkshopPool>,
    ) -> Self {
        Self {
            profiles,
            pool: Some(pool),
        }
    }
}

impl lumina_core::AgentInvoker for AcpAgentInvoker {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        if let Some(pool) = self.pool.as_ref() {
            return pool
                .submit(task.prompt, task.task_label, task.retry_task_label)
                .map_err(map_acp_error);
        }
        let model_selection = task
            .model_id
            .filter(|id| !id.trim().is_empty())
            .map(|model_id| lumina_acp::AcpSessionModelSelection {
                model_id,
                reasoning_effort: task
                    .reasoning_effort
                    .filter(|value| !value.trim().is_empty()),
            });
        lumina_acp::jobs::isolated::prompt_isolated_restricted(
            task.prompt,
            task.profile_id,
            self.profiles.clone(),
            model_selection,
            task.task_label,
        )
        .map_err(map_acp_error)
    }

    fn open_conversation<'a>(
        &'a self,
        task_label: Option<String>,
    ) -> Result<Box<dyn AgentConversation + 'a>, AgentTaskError> {
        let Some(pool) = self.pool.as_ref() else {
            let _ = task_label;
            return Ok(Box::new(AdapterOneShotConversation { invoker: self }));
        };
        pool.open_conversation(task_label).map_err(map_acp_error)
    }
}

struct AdapterOneShotConversation<'a> {
    invoker: &'a AcpAgentInvoker,
}

impl AgentConversation for AdapterOneShotConversation<'_> {
    fn prompt(&mut self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        lumina_core::AgentInvoker::invoke_isolated(self.invoker, task)
    }
}

/// Shared ACP→port error mapping for both pool and legacy paths.
fn map_acp_error(error: lumina_acp::AcpError) -> AgentTaskError {
    if error.code == lumina_acp::AcpErrorCode::NotConfigured {
        AgentTaskError::NotConfigured {
            details: error.details,
        }
    } else if error.code == lumina_acp::AcpErrorCode::NoOutput {
        AgentTaskError::NoOutput {
            details: error.details,
        }
    } else {
        AgentTaskError::Failed {
            details: error.details,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_kind_selects_explicit_mcp_profiles() {
        assert_eq!(
            mcp_profile_for_session(SessionKind::Chat),
            crate::mcp::McpToolProfile::Chat
        );
        assert_eq!(
            mcp_profile_for_session(SessionKind::Workshop),
            crate::mcp::McpToolProfile::NoTools
        );
        assert_eq!(
            mcp_profile_for_session(SessionKind::Chapter),
            crate::mcp::McpToolProfile::Chat
        );
    }

    #[test]
    fn chapter_mcp_servers_inject_chat_profile() {
        let root = std::env::temp_dir().join(format!(
            "lumina-chapter-mcp-config-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        let snapshot_path = root.join(".lumina").join("chapter-agent-context.json");
        let servers = AppSessionEnvironment.mcp_servers(&snapshot_path, SessionKind::Chapter);
        let env = servers
            .get(0)
            .and_then(|server| server.get("env"))
            .and_then(serde_json::Value::as_array)
            .expect("chapter MCP server environment");
        assert!(env.iter().any(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str)
                == Some(crate::mcp::TOOL_PROFILE_ENV)
                && entry.get("value").and_then(serde_json::Value::as_str) == Some("chat")
        }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn chapter_snapshot_contains_media_anchor_and_tool_capability() {
        let root = std::env::temp_dir().join(format!(
            "lumina-chapter-snapshot-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        let media = root.join("episode.mp4");
        let result = write_chapter_snapshot(&root, &media, 12_345, 60_000, Some("cache:subdl:en"))
            .expect("chapter snapshot should be written");
        let snapshot = read_snapshot(&result).expect("chapter snapshot should be readable");
        let anchor = snapshot.anchor.expect("chapter anchor");
        assert_eq!(anchor.media_path, media.to_string_lossy());
        assert_eq!(anchor.position_ms, 12_345);
        assert_eq!(anchor.duration_ms, Some(60_000));
        assert_eq!(anchor.subtitle_choice_id.as_deref(), Some("cache:subdl:en"));
        assert!(
            snapshot
                .capabilities
                .expect("chapter capabilities")
                .vision_capable
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn chapter_task_snapshot_contains_durable_scope() {
        let root = std::env::temp_dir().join(format!(
            "lumina-chapter-task-snapshot-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        let media = root.join("episode.mp4");
        let result = write_chapter_task_snapshot(
            &root,
            &media,
            12_345,
            60_000,
            Some("cache:subdl:en"),
            ChapterTaskContext {
                task_id: 7,
                attempt_id: 8,
                episode_id: 9,
                database_path: "C:\\data\\lumina.sqlite3".to_string(),
                media_path: media.to_string_lossy().into_owned(),
                duration_ms: 60_000,
                spoiler_boundary: "full_media".to_string(),
                prompt_version: "1.0".to_string(),
            },
        )
        .expect("chapter task snapshot should be written");
        let scope = crate::mcp::read_chapter_task_context(&result)
            .expect("chapter task snapshot should be readable")
            .expect("chapter task scope");
        assert_eq!(scope.task_id, 7);
        assert_eq!(scope.attempt_id, 8);
        assert_eq!(scope.episode_id, 9);
        assert_eq!(scope.database_path, "C:\\data\\lumina.sqlite3");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn chapter_session_sync_preserves_scoped_snapshot() {
        let root = std::env::temp_dir().join(format!(
            "lumina-chapter-sync-adapter-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        let snapshot_path = root.join(".lumina").join("chapter-agent-context.json");
        let context = ChapterTaskContext {
            task_id: 51,
            attempt_id: 52,
            episode_id: 53,
            database_path: r"C:\data\lumina.sqlite3".into(),
            media_path: r"C:\videos\episode.mkv".into(),
            duration_ms: 90_000,
            spoiler_boundary: "episode".into(),
            prompt_version: "chapter-v1".into(),
        };

        write_scoped_snapshot(&snapshot_path, &LuminaMcpSnapshot::empty(), &context)
            .expect("write scoped snapshot");
        AppSessionEnvironment
            .sync_snapshot(&snapshot_path, true)
            .expect("sync chapter snapshot");

        assert_eq!(
            crate::mcp::read_chapter_task_context(&snapshot_path).expect("read chapter scope"),
            Some(context)
        );
        let snapshot = read_snapshot(&snapshot_path).expect("read synced snapshot");
        assert!(
            snapshot
                .capabilities
                .expect("chapter capabilities")
                .vision_capable
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
