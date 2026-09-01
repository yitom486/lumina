mod server;
mod snapshot;

pub use snapshot::{
    snapshot_path_for_cwd, write_snapshot, LuminaMcpSnapshot, CONTEXT_FILE_ENV,
    SNAPSHOT_RELATIVE_PATH,
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
        "type": "stdio",
        "name": "lumina",
        "command": executable,
        "args": [MCP_SUBCOMMAND],
        "env": {
            CONTEXT_FILE_ENV: snapshot_path.to_string_lossy(),
        }
    })
}

pub fn lumina_mcp_servers(snapshot_path: &Path) -> Value {
    json!([lumina_mcp_server_entry(snapshot_path)])
}
