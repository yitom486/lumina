//! App-side ACP adapters (M5).
//!
//! Warm chat-snapshot state and all MCP/library IO live here so `lumina-acp`
//! stays a generic Client. Spawn paths (`session/new|resume`) additionally go
//! through [`AppSessionEnvironment`], installed once at startup.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lumina_acp::{AcpError, SessionEnvironment, VideoPromptContext};
use lumina_core::{AgentConversation, AgentTaskError, IsolatedAgentTask};

use crate::library::MediaLibraryService;
use crate::mcp::{
    read_snapshot, snapshot_path_for_cwd, sync_snapshot_capabilities, write_snapshot,
    LuminaMcpSnapshot, PromptSnapshotState,
};

/// MCP-backed [`SessionEnvironment`] for ACP spawn paths.
pub struct AppSessionEnvironment;

impl SessionEnvironment for AppSessionEnvironment {
    fn snapshot_path(&self, cwd: &Path) -> PathBuf {
        snapshot_path_for_cwd(cwd)
    }

    fn sync_snapshot(&self, snapshot_path: &Path, vision_capable: bool) -> Result<(), String> {
        sync_snapshot_capabilities(snapshot_path, vision_capable)
    }

    fn mcp_servers(&self, snapshot_path: &Path, isolated: bool) -> serde_json::Value {
        // Chat selects `Chat`; isolated AI tasks select `NoTools` so the
        // server lists nothing and never loads Chat snapshot state.
        let profile = if isolated {
            crate::mcp::McpToolProfile::NoTools
        } else {
            crate::mcp::McpToolProfile::Chat
        };
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
