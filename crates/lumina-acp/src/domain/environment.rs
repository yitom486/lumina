//! App-provided session environment for ACP spawn paths.
//!
//! `lumina-acp` never touches the Lumina MCP snapshot or file library
//! directly. The app installs one [`SessionEnvironment`] at startup; all
//! `session/new|resume` wiring and snapshot capability IO go through it.
//! Chat snapshot build/write/reset live in the app adapter instead.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde_json::Value;

use crate::error::AcpError;

/// Generalized MCP/session surroundings, implemented once by the app.
pub trait SessionEnvironment: Send + Sync + 'static {
    /// Snapshot file location for a session cwd.
    fn snapshot_path(&self, cwd: &Path) -> PathBuf;
    /// Record session capabilities (e.g. vision) into the snapshot file.
    fn sync_snapshot(&self, snapshot_path: &Path, vision_capable: bool) -> Result<(), String>;
    /// Generalized MCP server spec for `session/new|resume`.
    /// `isolated` marks short-lived tool-free tasks (translation/polishing):
    /// the app advertises the `NoTools` profile so the server never serves
    /// tools or loads Chat state. Chat sessions pass `false` (`Chat`).
    fn mcp_servers(&self, snapshot_path: &Path, isolated: bool) -> Value;
    /// Vision flag previously recorded for a workspace, if any.
    fn snapshot_vision_capable(&self, workspace: &Path) -> Option<bool>;
}

static DEFAULT_ENVIRONMENT: OnceLock<Arc<dyn SessionEnvironment>> = OnceLock::new();

/// Called once by the app at startup. Later calls keep the first install.
pub fn set_default_environment(env: Arc<dyn SessionEnvironment>) -> bool {
    DEFAULT_ENVIRONMENT.set(env).is_ok()
}

pub(crate) fn session_env() -> Result<Arc<dyn SessionEnvironment>, AcpError> {
    DEFAULT_ENVIRONMENT
        .get()
        .cloned()
        .ok_or_else(|| AcpError::internal(Some("ACP session environment is not configured")))
}
