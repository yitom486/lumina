mod build;
mod server;
mod snapshot;
mod tools;

pub use build::PromptSnapshotState;
pub use snapshot::{
    read_snapshot, snapshot_path_for_cwd, sync_snapshot_capabilities, write_snapshot,
    AgentCapabilities, LuminaMcpSnapshot, PlaybackLite, PromptAnchor, SessionPolicy,
    CONTEXT_FILE_ENV, LIBRARY_WARM_EVERY, SNAPSHOT_RELATIVE_PATH, SNAPSHOT_SCHEMA_VERSION,
};

use std::path::Path;

use serde_json::{json, Value};

/// Spawn args for the in-process Lumina MCP stdio server.
pub const MCP_SUBCOMMAND: &str = "--lumina-mcp";

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
