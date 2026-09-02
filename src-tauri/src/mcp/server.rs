//! Minimal MCP stdio server exposing Lumina context tools.

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use super::snapshot::{read_snapshot, resolve_snapshot_path, LuminaMcpSnapshot};
use super::tools::{handle_tool_call, vision_capable};

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
            "tools/list" => match load_snapshot() {
                Ok(snapshot) => success(id, tools_list_result(&snapshot)),
                Err(message) => error(id, -32000, &message),
            },
            "tools/call" => match load_snapshot() {
                Ok(snapshot) => match handle_tool_call_request(&snapshot, &params) {
                    Ok(result) => success(id, result),
                    Err(message) => {
                        tracing::warn!(
                            tool = params.get("name").and_then(|value| value.as_str()).unwrap_or(""),
                            reason = %message,
                            "lumina MCP tools/call returned business error"
                        );
                        success(id, tool_error_result(&message))
                    }
                },
                Err(message) => {
                    tracing::warn!(reason = %message, "lumina MCP snapshot unavailable");
                    error(id, -32000, &message)
                }
            },
            "ping" => success(id, json!({})),
            _ if id.is_null() => continue,
            other => error(id, -32601, &format!("Method not found: {other}")),
        };
        writeln!(
            stdout,
            "{}",
            serde_json::to_string(&response).unwrap_or_default()
        )
        .map_err(|error| format!("stdout write: {error}"))?;
        stdout
            .flush()
            .map_err(|error| format!("stdout flush: {error}"))?;
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

fn tools_list_result(snapshot: &LuminaMcpSnapshot) -> Value {
    let mut tools = vec![
        json!({
            "name": "lumina_get_playback_context",
            "description": "Return frozen playback anchor, progress, chapter title, and nearby notes for the current prompt turn.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "lumina_get_library_context",
            "description": "Return series metadata plus the current episode overview/plot. Loads from warm cache when present, otherwise reads local metadata on demand.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "lumina_get_episode_index",
            "description": "List episode titles and overviews for every episode in the current series group.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "lumina_get_transcript_window",
            "description": "Return subtitle lines around a time point on the current media file. Defaults to the frozen prompt anchor; optional centerMs or atSec overrides the center. Window defaults to 60 seconds before and after (each side up to 300 seconds).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "centerMs": { "type": "integer", "minimum": 0 },
                    "atSec": { "type": "integer", "minimum": 0 },
                    "beforeSec": { "type": "integer", "minimum": 0, "maximum": 300 },
                    "afterSec": { "type": "integer", "minimum": 0, "maximum": 300 },
                    "radiusSec": { "type": "integer", "minimum": 0, "maximum": 300 }
                },
                "additionalProperties": false
            }
        }),
    ];

    if vision_capable(snapshot) {
        tools.push(json!({
            "name": "lumina_capture_frames",
            "description": "Capture temporary JPEG frames (~1 per second) around the frozen anchor time. Default is a single frame at the anchor. Use radiusSec or beforeSec/afterSec (each capped at 7s, max 15 frames). Files are discarded after the tool returns.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "beforeSec": { "type": "integer", "minimum": 0, "maximum": 7 },
                    "afterSec": { "type": "integer", "minimum": 0, "maximum": 7 },
                    "radiusSec": { "type": "integer", "minimum": 0, "maximum": 7 }
                },
                "additionalProperties": false
            }
        }));
    }

    json!({ "tools": tools })
}

fn handle_tool_call_request(snapshot: &LuminaMcpSnapshot, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call missing name".to_string())?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    handle_tool_call(snapshot, name, &args)
}

fn tool_error_result(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
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
    use crate::mcp::snapshot::{AgentCapabilities, LuminaMcpSnapshot, SNAPSHOT_SCHEMA_VERSION};

    #[test]
    fn tools_list_contains_core_tools() {
        let snapshot = LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: None,
            playback: None,
            library: None,
            session: None,
            capabilities: Some(AgentCapabilities {
                vision_capable: false,
            }),
            updated_at_ms: 0,
        };
        let tools = tools_list_result(&snapshot)
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"lumina_get_playback_context"));
        assert!(names.contains(&"lumina_get_transcript_window"));
        assert!(!names.contains(&"lumina_capture_frames"));
    }

    #[test]
    fn tools_list_includes_capture_when_vision_enabled() {
        let snapshot = LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: None,
            playback: None,
            library: None,
            session: None,
            capabilities: Some(AgentCapabilities {
                vision_capable: true,
            }),
            updated_at_ms: 0,
        };
        let tools = tools_list_result(&snapshot)
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"lumina_capture_frames"));
    }
}
