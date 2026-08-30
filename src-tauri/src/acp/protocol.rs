//! Minimal ACP JSON-RPC helpers (line-delimited over stdio).

use serde_json::{json, Value};

use crate::acp::error::AcpError;

pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

pub fn initialize_params() -> Value {
    json!({
        "protocolVersion": 1,
        "clientInfo": {
            "name": "lumina",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false,
        },
    })
}

pub fn session_new_params(cwd: Option<&str>) -> Value {
    let mut params = json!({
        "cwd": cwd.unwrap_or("."),
        "mcpServers": [],
    });
    // Some adapters accept empty mcpServers; keep shape stable.
    if let Some(obj) = params.as_object_mut() {
        obj.entry("cwd".to_string())
            .or_insert_with(|| json!(cwd.unwrap_or(".")));
    }
    params
}

pub fn session_prompt_params(session_id: &str, text: &str) -> Value {
    json!({
        "sessionId": session_id,
        "prompt": [
            {
                "type": "text",
                "text": text,
            }
        ],
    })
}

pub fn encode_line(value: &Value) -> Result<String, AcpError> {
    serde_json::to_string(value).map_err(|error| {
        tracing::warn!(%error, "failed to serialize ACP request");
        AcpError::protocol(Some(&format!("serialize ACP request: {error}")))
    })
}

/// Extract assistant-visible text fragments from an ACP notification / result.
pub fn extract_agent_text(value: &Value) -> Option<String> {
    // ACP v1: session/update → update.sessionUpdate=agent_message_chunk,
    // with one ContentBlock object in `content`.
    if let Some(update) = value.pointer("/params/update") {
        if update.get("sessionUpdate").and_then(Value::as_str) == Some("agent_message_chunk") {
            return content_blocks_text(update.get("content"));
        }
    }

    if let Some(text) = value
        .pointer("/params/content")
        .and_then(|c| content_blocks_text(Some(c)))
    {
        return Some(text);
    }

    // session/prompt result contains only stopReason; it is control metadata,
    // never assistant-visible text.
    None
}

fn content_blocks_text(content: Option<&Value>) -> Option<String> {
    let content = content?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    if content.get("type").and_then(Value::as_str) == Some("text") {
        return content
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    let arr = content.as_array()?;
    let mut parts = Vec::new();
    for item in arr {
        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                parts.push(text.to_string());
            }
        } else if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
            parts.push(text.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(""))
    }
}

pub fn parse_session_id(value: &Value) -> Option<String> {
    value
        .pointer("/result/sessionId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            value
                .get("result")
                .and_then(|r| r.get("sessionId"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
}

pub fn is_error_response(value: &Value) -> Option<String> {
    let err = value.get("error")?;
    let message = err
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("ACP protocol error");
    Some(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_initialize_roundtrip_shape() {
        let req = request(1, "initialize", initialize_params());
        let line = encode_line(&req).expect("encode");
        assert!(line.contains("initialize"));
        assert!(line.contains("lumina"));
    }

    #[test]
    fn extract_text_from_content_blocks() {
        let value = json!({
            "params": {
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "你好" }
                }
            }
        });
        assert_eq!(extract_agent_text(&value).as_deref(), Some("你好"));
    }

    #[test]
    fn stop_reason_is_not_assistant_text() {
        let value = json!({
            "id": 3,
            "result": { "stopReason": "end_turn" }
        });
        assert_eq!(extract_agent_text(&value), None);
    }

    #[test]
    fn ignores_non_message_session_updates() {
        let value = json!({
            "method": "session/update",
            "params": {
                "update": {
                    "sessionUpdate": "tool_call",
                    "content": { "type": "text", "text": "internal tool output" }
                }
            }
        });
        assert_eq!(extract_agent_text(&value), None);
    }
}
