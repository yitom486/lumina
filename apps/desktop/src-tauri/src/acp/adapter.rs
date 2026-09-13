//! App-side ACP adapters (M5).
//!
//! Warm chat-snapshot state and all MCP/library IO live here so `lumina-acp`
//! stays a generic Client. Spawn paths (`session/new|resume`) additionally go
//! through [`AppSessionEnvironment`], installed once at startup.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lumina_acp::{AcpError, SessionEnvironment, VideoPromptContext};

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

    fn mcp_servers(&self, snapshot_path: &Path) -> serde_json::Value {
        crate::mcp::lumina_mcp_servers(snapshot_path)
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
pub fn build_prompt_snapshot(
    snapshots: &Mutex<PromptSnapshotState>,
    library: &MediaLibraryService,
    context: Option<&VideoPromptContext>,
    vision_capable: bool,
) -> Result<LuminaMcpSnapshot, AcpError> {
    let mut guard = snapshots
        .lock()
        .map_err(|_| AcpError::internal(Some("prompt snapshot mutex poisoned")))?;
    guard
        .next_snapshot(context, library, vision_capable)
        .map_err(|error| AcpError::internal(error.details.as_deref()))
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
    let workspace = crate::acp::paths::resolve_session_cwd(cwd_hint)?;
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
pub struct AcpAgentInvoker {
    profiles: lumina_acp::AgentProfilesHint,
}

impl AcpAgentInvoker {
    pub fn new(profiles: lumina_acp::AgentProfilesHint) -> Self {
        Self { profiles }
    }
}

impl lumina_core::AgentInvoker for AcpAgentInvoker {
    fn invoke_isolated(
        &self,
        task: lumina_core::IsolatedAgentTask,
    ) -> Result<String, lumina_core::AgentTaskError> {
        use lumina_core::AgentTaskError;
        let model_selection = task
            .model_id
            .filter(|id| !id.trim().is_empty())
            .map(|model_id| lumina_acp::AcpSessionModelSelection {
                model_id,
                reasoning_effort: task
                    .reasoning_effort
                    .filter(|value| !value.trim().is_empty()),
            });
        lumina_acp::AcpService::prompt_isolated_restricted(
            task.prompt,
            task.profile_id,
            self.profiles.clone(),
            model_selection,
        )
        .map_err(|error| {
            if error.code == lumina_acp::AcpErrorCode::NotConfigured {
                AgentTaskError::NotConfigured {
                    details: error.details,
                }
            } else {
                AgentTaskError::Failed {
                    details: error.details,
                }
            }
        })
    }
}
