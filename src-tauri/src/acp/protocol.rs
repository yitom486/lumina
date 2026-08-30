//! ACP JSON-RPC helpers (line-delimited over stdio).
//! Covers Client→Agent methods/notifications and Agent→Client response helpers.

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

pub fn notification(method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

pub fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

pub fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
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
    json!({
        "cwd": cwd.unwrap_or("."),
        "mcpServers": [],
    })
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

pub fn session_cancel_params(session_id: &str) -> Value {
    json!({ "sessionId": session_id })
}

pub fn session_close_params(session_id: &str) -> Value {
    json!({ "sessionId": session_id })
}

pub fn authenticate_params(method_id: &str) -> Value {
    json!({ "methodId": method_id })
}

pub fn encode_line(value: &Value) -> Result<String, AcpError> {
    serde_json::to_string(value).map_err(|error| {
        tracing::warn!(%error, "failed to serialize ACP JSON");
        AcpError::protocol(Some(&format!("serialize ACP JSON: {error}")))
    })
}

#[derive(Debug, Clone, Default)]
pub struct InitializeResult {
    pub protocol_version: Option<u64>,
    pub agent_name: Option<String>,
    pub auth_methods: Vec<AuthMethod>,
    pub supports_session_close: bool,
    pub load_session: bool,
}

#[derive(Debug, Clone)]
pub struct AuthMethod {
    pub id: String,
    pub name: String,
}

pub fn parse_initialize_result(value: &Value) -> InitializeResult {
    let result = value.get("result").unwrap_or(value);
    let protocol_version = result
        .get("protocolVersion")
        .and_then(Value::as_u64)
        .or_else(|| result.get("protocolVersion").and_then(Value::as_i64).map(|v| v as u64));

    let agent_name = result
        .pointer("/agentInfo/name")
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut auth_methods = Vec::new();
    if let Some(arr) = result.get("authMethods").and_then(Value::as_array) {
        for item in arr {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                continue;
            }
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id.as_str())
                .to_string();
            auth_methods.push(AuthMethod { id, name });
        }
    }

    let supports_session_close = result
        .pointer("/agentCapabilities/sessionCapabilities/close")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || result
            .pointer("/agentCapabilities/session/close")
            .and_then(Value::as_bool)
            .unwrap_or(false);

    let load_session = result
        .pointer("/agentCapabilities/loadSession")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    InitializeResult {
        protocol_version,
        agent_name,
        auth_methods,
        supports_session_close,
        load_session,
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

pub fn parse_stop_reason(value: &Value) -> Option<String> {
    value
        .pointer("/result/stopReason")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub fn is_error_response(value: &Value) -> Option<String> {
    let err = value.get("error")?;
    let message = err
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("ACP protocol error");
    Some(message.to_string())
}

/// Classify an inbound ACP line.
#[derive(Debug)]
pub enum Inbound {
    /// JSON-RPC response to our request (`id` present, no `method`).
    Response { id: u64, value: Value },
    /// Agent→Client request (`method` + `id`).
    AgentRequest {
        id: Value,
        method: String,
        params: Value,
    },
    /// Notification (`method`, no `id`).
    Notification { method: String, params: Value },
    Other(Value),
}

pub fn classify_inbound(value: Value) -> Inbound {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string);
    let id = value.get("id").cloned();

    match (method, id) {
        (Some(method), Some(id)) => Inbound::AgentRequest {
            id,
            method,
            params: value.get("params").cloned().unwrap_or(json!({})),
        },
        (Some(method), None) => Inbound::Notification {
            method,
            params: value.get("params").cloned().unwrap_or(json!({})),
        },
        (None, Some(id)) => {
            let id_u = id.as_u64().or_else(|| id.as_i64().map(|v| v as u64)).unwrap_or(0);
            Inbound::Response {
                id: id_u,
                value,
            }
        }
        _ => Inbound::Other(value),
    }
}

/// Extract assistant-visible text from `session/update` agent_message_chunk.
pub fn extract_agent_text(value: &Value) -> Option<String> {
    let update = session_update_payload(value)?;
    if update.get("sessionUpdate").and_then(Value::as_str) == Some("agent_message_chunk") {
        return content_blocks_text(update.get("content"));
    }
    None
}

pub fn extract_thought_text(value: &Value) -> Option<String> {
    let update = session_update_payload(value)?;
    if update.get("sessionUpdate").and_then(Value::as_str) == Some("agent_thought_chunk") {
        return content_blocks_text(update.get("content"));
    }
    None
}

pub fn extract_tool_call(value: &Value) -> Option<ToolCallInfo> {
    let update = session_update_payload(value)?;
    let kind = update.get("sessionUpdate").and_then(Value::as_str)?;
    if kind != "tool_call" && kind != "tool_call_update" {
        return None;
    }
    Some(ToolCallInfo {
        update_kind: kind.to_string(),
        tool_call_id: update
            .get("toolCallId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        title: update
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        status: update
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string),
        kind: update
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

pub fn extract_plan_summary(value: &Value) -> Option<String> {
    let update = session_update_payload(value)?;
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("plan") {
        return None;
    }
    let entries = update.get("entries").and_then(Value::as_array)?;
    let mut lines = Vec::new();
    for entry in entries {
        let content = entry
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !content.is_empty() {
            let status = entry
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("pending");
            lines.push(format!("[{status}] {content}"));
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn session_update_payload(value: &Value) -> Option<&Value> {
    if value.get("method").and_then(Value::as_str) == Some("session/update") {
        return value.pointer("/params/update");
    }
    value.pointer("/params/update")
}

#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub update_kind: String,
    pub tool_call_id: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub kind: Option<String>,
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

/// Auto-resolve permission: prefer allow_* option, else cancelled.
pub fn permission_auto_result(params: &Value, canceling: bool) -> Value {
    if canceling {
        return json!({ "outcome": { "outcome": "cancelled" } });
    }
    let options = params
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for opt in &options {
        let kind = opt.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind.starts_with("allow") {
            if let Some(option_id) = opt.get("optionId").and_then(Value::as_str) {
                return json!({
                    "outcome": { "outcome": "selected", "optionId": option_id }
                });
            }
        }
    }
    json!({ "outcome": { "outcome": "cancelled" } })
}

/// Build JSON-RPC response for an Agent→Client request.
pub fn handle_agent_request(method: &str, id: Value, params: &Value, canceling: bool) -> Value {
    match method {
        "session/request_permission" => {
            success_response(id, permission_auto_result(params, canceling))
        }
        "fs/read_text_file" | "fs/write_text_file" => error_response(
            id,
            -32601,
            "Lumina ACP client does not expose filesystem methods",
        ),
        "terminal/create"
        | "terminal/output"
        | "terminal/release"
        | "terminal/wait_for_exit"
        | "terminal/kill" => error_response(
            id,
            -32601,
            "Lumina ACP client does not expose terminal methods",
        ),
        "elicitation/create" => {
            success_response(id, json!({ "outcome": { "outcome": "cancelled" } }))
        }
        other => {
            tracing::warn!(method = other, "unsupported Agent→Client ACP method");
            error_response(id, -32601, &format!("Method not found: {other}"))
        }
    }
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
            "method": "session/update",
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
        assert_eq!(parse_stop_reason(&value).as_deref(), Some("end_turn"));
    }

    #[test]
    fn permission_prefers_allow_option() {
        let params = json!({
            "options": [
                { "optionId": "deny", "kind": "reject_once", "name": "Deny" },
                { "optionId": "allow", "kind": "allow_once", "name": "Allow" }
            ]
        });
        let result = permission_auto_result(&params, false);
        assert_eq!(
            result.pointer("/outcome/optionId").and_then(Value::as_str),
            Some("allow")
        );
    }

    #[test]
    fn cancel_notification_shape() {
        let n = notification("session/cancel", session_cancel_params("s1"));
        assert!(n.get("id").is_none());
        assert_eq!(n.get("method").and_then(Value::as_str), Some("session/cancel"));
    }

    #[test]
    fn classify_agent_request() {
        let value = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "session/request_permission",
            "params": { "sessionId": "s" }
        });
        match classify_inbound(value) {
            Inbound::AgentRequest { method, .. } => {
                assert_eq!(method, "session/request_permission");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
