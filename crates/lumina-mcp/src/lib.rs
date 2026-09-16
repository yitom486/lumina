//! Lumina MCP server, snapshot, and tools (library crate).
//!
//! Tool provider for agents; never manages ACP sessions. Tool visibility is
//! governed by [`policy::McpToolProfile`]/[`policy::ToolPolicy`].

mod build;
mod executor;
mod policy;
mod server;
mod snapshot;
mod tools;

pub use build::{McpPromptContext, PromptSnapshotState, SnapshotBuildResult};
pub use policy::{tool_profile_from_env, McpToolProfile, ToolPolicy, TOOL_PROFILE_ENV};
pub use snapshot::{
    read_snapshot, snapshot_path_for_cwd, sync_snapshot_capabilities, write_snapshot,
    AgentCapabilities, CurrentEpisodeLite, LuminaMcpSnapshot, OnlineMediaSnapshot, PromptAnchor,
    SessionPolicy, CONTEXT_FILE_ENV, LIBRARY_WARM_EVERY, SNAPSHOT_RELATIVE_PATH,
    SNAPSHOT_SCHEMA_VERSION,
};

use std::path::Path;

use serde_json::{json, Value};

/// Spawn args for the in-process Lumina MCP stdio server.
pub const MCP_SUBCOMMAND: &str = "--lumina-mcp";

/// Optional append-only diagnostic log path supplied by the desktop host.
///
/// MCP runs as a child of the Agent harness, so its stderr is not guaranteed
/// to reach Lumina's tracing subscriber. The server keeps stderr diagnostics
/// for direct probes and mirrors them to this file when the host provides it.
pub const DIAGNOSTIC_LOG_ENV: &str = "LUMINA_MCP_DIAGNOSTIC_LOG";

/// Per-MCP-child correlation id supplied by the desktop ACP host.
///
/// This lets the host's `session/new|resume` registration log be matched to
/// the child process that later emits `initialize` and `tools/list` events.
pub const DIAGNOSTIC_ID_ENV: &str = "LUMINA_MCP_DIAGNOSTIC_ID";

pub fn run_if_invoked() -> bool {
    if std::env::args().any(|arg| arg == MCP_SUBCOMMAND) {
        if let Err(error) = server::run_stdio_server() {
            eprintln!("lumina mcp server failed: {error}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }
    false
}

pub fn lumina_mcp_server_entry(snapshot_path: &Path) -> Value {
    let executable = std::env::current_exe()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| "lumina".into());
    json!({
        "name": "lumina",
        "command": executable,
        "args": [MCP_SUBCOMMAND],
        "env": [
            {
                "name": CONTEXT_FILE_ENV,
                "value": snapshot_path.to_string_lossy(),
            }
        ]
    })
}

pub fn lumina_mcp_servers(snapshot_path: &Path) -> Value {
    json!([lumina_mcp_server_entry(snapshot_path)])
}
