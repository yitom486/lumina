//! Minimal MCP stdio server exposing Lumina context tools.

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use super::snapshot::{read_snapshot, resolve_snapshot_path, LuminaMcpSnapshot};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn run_stdio_server() -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("stdin read: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value =
            serde_json::from_str(&line).map_err(|error| format!("invalid json: {error}"))?;
        if request.get("method").and_then(Value::as_str) == Some("notifications/initialized") {
            continue;
        }
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let response = match method {
            "initialize" => success(id, initialize_result(params)),
            "tools/list" => success(id, tools_list_result()),
            "tools/call" => match handle_tool_call(&params) {
                Ok(result) => success(id, result),
                Err(message) => error(id, -32000, &message),
            },
            "ping" => success(id, json!({})),
            _ if id.is_null() => continue,
            other => error(id, -32601, &format!("Method not found: {other}")),
        };
        writeln!(stdout, "{}", serde_json::to_string(&response).unwrap_or_default())
            .map_err(|error| format!("stdout write: {error}"))?;
        stdout.flush().map_err(|error| format!("stdout flush: {error}"))?;
    }
    Ok(())
}

fn initialize_result(params: Value) -> Value {
    let _ = params;
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "lumina", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn tools_list_result() -> Value {
    json!({
        "tools": [
            {
                "name": "lumina_get_playback_context",
                "description": "Return current Lumina playback snapshot: media path/title, progress, chapter, transcript excerpt, nearby notes.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "lumina_get_library_context",
                "description": "Return merged TMDb + Wikipedia metadata for the media currently open in Lumina (synopsis, characters, episode plot, attribution).",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            }
        ]
    })
}

fn handle_tool_call(params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call missing name".to_string())?;
    let snapshot = load_snapshot()?;
    let payload = match name {
        "lumina_get_playback_context" => json!({ "playback": snapshot.playback }),
        "lumina_get_library_context" => json!({ "library": snapshot.library }),
        other => return Err(format!("Unknown tool: {other}")),
    };
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    }))
}

fn load_snapshot() -> Result<LuminaMcpSnapshot, String> {
    let path = resolve_snapshot_path()
        .ok_or_else(|| "LUMINA_MCP_CONTEXT_FILE or cwd unavailable".to_string())?;
    read_snapshot(&path)
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_contains_lumina_tools() {
        let tools = tools_list_result()
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"lumina_get_playback_context"));
        assert!(names.contains(&"lumina_get_library_context"));
    }
}
