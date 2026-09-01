//! ACP JSON-RPC helpers (line-delimited over stdio).
//! Covers Client→Agent methods/notifications and Agent→Client response helpers.

use serde_json::{json, Value};

use crate::acp::discover::codex_config_present;
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
    initialize_params_with_tools(true)
}

/// Metadata-only sessions never need filesystem or terminal capabilities.
/// Advertise neither so an Agent cannot treat the media directory as a tool
/// workspace while resolving filenames.
pub fn initialize_params_restricted() -> Value {
    initialize_params_with_tools(false)
}

#[cfg(test)]
mod initialize_tests {
    use super::*;

    #[test]
    fn restricted_initialize_advertises_no_tools() {
        let params = initialize_params_restricted();
        assert_eq!(params["clientCapabilities"], json!({}));
    }
}

fn initialize_params_with_tools(tool_access: bool) -> Value {
    json!({
        "protocolVersion": 1,
        "clientInfo": {
            "name": "lumina",
            "title": "Lumina",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "clientCapabilities": if tool_access {
            json!({ "fs": { "readTextFile": true, "writeTextFile": true }, "terminal": true })
        } else {
            json!({})
        },
    })
}

/// `cwd` MUST be an absolute path (ACP session-setup).
pub fn session_new_params(cwd: &str, mcp_servers: Value) -> Value {
    json!({
        "cwd": cwd,
        "mcpServers": mcp_servers,
    })
}

pub fn session_prompt_params(session_id: &str, text: &str) -> Value {
    crate::acp::context::session_prompt_params(session_id, text, None)
}

pub fn session_resume_params(session_id: &str, cwd: &str, mcp_servers: Value) -> Value {
    json!({
        "sessionId": session_id,
        "cwd": cwd,
        "mcpServers": mcp_servers,
    })
}

pub fn session_cancel_params(session_id: &str) -> Value {
    json!({ "sessionId": session_id })
}

pub fn session_close_params(session_id: &str) -> Value {
    json!({ "sessionId": session_id })
}

pub fn session_set_config_option_params(session_id: &str, config_id: &str, value: &str) -> Value {
    json!({
        "sessionId": session_id,
        "configId": config_id,
        "value": value,
    })
}

pub fn authenticate_params(method_id: &str) -> Value {
    json!({ "methodId": method_id })
}

/// Pick an auth method compatible with local Codex setup (ChatGPT login vs API key).
pub fn pick_auth_method(init: &InitializeResult) -> Option<&AuthMethod> {
    if init.auth_methods.is_empty() {
        return None;
    }
    let order: &[&str] = if codex_config_present() {
        &["chat-gpt", "chat-gpt-device-code", "gateway", "api-key"]
    } else if std::env::var("OPENAI_API_KEY").is_ok() {
        &["api-key", "chat-gpt", "chat-gpt-device-code", "gateway"]
    } else {
        &["chat-gpt", "chat-gpt-device-code", "api-key", "gateway"]
    };
    for id in order {
        if let Some(method) = init.auth_methods.iter().find(|method| method.id == *id) {
            return Some(method);
        }
    }
    init.auth_methods.first()
}

fn capability_present(value: &Value, pointer: &str) -> bool {
    match value.pointer(pointer) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => true,
    }
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
    pub supports_session_resume: bool,
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
        .or_else(|| {
            result
                .get("protocolVersion")
                .and_then(Value::as_i64)
                .map(|v| v as u64)
        });

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

    // Spec: advertising `sessionCapabilities.close` as `{}` (or true) means supported.
    let supports_session_close =
        capability_present(result, "/agentCapabilities/sessionCapabilities/close")
            || capability_present(result, "/agentCapabilities/session/close");

    let supports_session_resume =
        capability_present(result, "/agentCapabilities/sessionCapabilities/resume");

    let load_session = result
        .pointer("/agentCapabilities/loadSession")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    InitializeResult {
        protocol_version,
        agent_name,
        auth_methods,
        supports_session_close,
        supports_session_resume,
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

pub fn parse_session_model_options(value: &Value) -> crate::acp::AcpSessionModelOptions {
    let result = value.get("result").unwrap_or(value);
    let mut options = crate::acp::AcpSessionModelOptions::default();
    let Some(config_options) = result.get("configOptions").and_then(Value::as_array) else {
        return options;
    };
    for option in config_options {
        let id = option.get("id").and_then(Value::as_str);
        let current = option
            .get("currentValue")
            .and_then(Value::as_str)
            .map(str::to_string);
        let values = option
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let value = item.get("value")?.as_str()?.trim();
                (!value.is_empty()).then(|| crate::acp::AcpSessionOption {
                    value: value.to_string(),
                    name: item
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or(value)
                        .to_string(),
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .filter(|description| !description.trim().is_empty())
                        .map(str::to_string),
                })
            })
            .collect::<Vec<_>>();
        match id {
            Some("model") => {
                options.models = values;
                options.current_model_id = current;
            }
            Some("reasoning_effort") => {
                options.reasoning_efforts = values;
                options.current_reasoning_effort = current;
            }
            _ => {}
        }
    }
    options
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
    Response {
        id: u64,
        value: Value,
    },
    /// Agent→Client request (`method` + `id`).
    AgentRequest {
        id: Value,
        method: String,
        params: Value,
    },
    /// Notification (`method`, no `id`).
    Notification {
        method: String,
        params: Value,
    },
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
            let id_u = id
                .as_u64()
                .or_else(|| id.as_i64().map(|v| v as u64))
                .unwrap_or(0);
            Inbound::Response { id: id_u, value }
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
        detail: extract_tool_call_detail(update),
        append_detail: false,
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
    pub detail: Option<String>,
    pub append_detail: bool,
}

/// Extract user-visible detail from a tool-call update payload.
pub fn extract_tool_call_detail(update: &Value) -> Option<String> {
    if let Some(text) = extract_tool_call_content_text(update) {
        let sanitized = sanitize_tool_detail(&text);
        if !sanitized.is_empty() {
            return Some(sanitized);
        }
    }
    update
        .get("rawOutput")
        .and_then(value_to_plain_text)
        .map(|text| sanitize_tool_detail(&text))
        .filter(|text| !text.is_empty())
}

pub fn extract_tool_call_content_text(update: &Value) -> Option<String> {
    let content = update.get("content")?;
    if content.is_null() {
        return None;
    }
    if let Some(items) = content.as_array() {
        let mut parts = Vec::new();
        for item in items {
            if let Some(text) = tool_call_content_item_text(item) {
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
        }
        if !parts.is_empty() {
            return Some(parts.join("\n"));
        }
    }
    content_blocks_text(Some(content))
}

pub fn extract_tool_call_content_chunk(value: &Value) -> Option<(String, String)> {
    let update = session_update_payload(value)?;
    if update.get("sessionUpdate").and_then(Value::as_str)? != "tool_call_content_chunk" {
        return None;
    }
    let tool_call_id = update
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let content = update.get("content")?;
    let text = tool_call_content_item_text(content)?;
    Some((tool_call_id, sanitize_tool_detail(&text)))
}

pub fn sanitize_tool_detail(input: &str) -> String {
    let collapsed = input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("；");
    if collapsed.is_empty() {
        return String::new();
    }
    let redacted = redact_absolute_paths(&collapsed);
    let capped = truncate_chars(&redacted, 220);
    if looks_like_low_level_tool_error(&capped) {
        "工具执行未成功，Agent 将尝试其他方式继续".into()
    } else {
        capped
    }
}

fn tool_call_content_item_text(item: &Value) -> Option<String> {
    match item.get("type").and_then(Value::as_str) {
        Some("content") => content_blocks_text(item.get("content")),
        Some("text") => item
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => content_blocks_text(Some(item)),
    }
}

fn value_to_plain_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Null => None,
        other => serde_json::to_string(other).ok(),
    }
}

fn redact_absolute_paths(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while !rest.is_empty() {
        if let Some((_, len)) = match_absolute_path_prefix(rest) {
            out.push_str("[路径]");
            rest = &rest[len..];
        } else {
            let ch = rest.chars().next().unwrap();
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

fn match_absolute_path_prefix(input: &str) -> Option<(&str, usize)> {
    if input.len() >= 3 {
        let bytes = input.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
            let mut end = 3;
            for (offset, ch) in input[3..].char_indices() {
                if ch.is_whitespace() || matches!(ch, '"' | '\'' | ')' | '(' | ',' | ';') {
                    break;
                }
                end = 3 + offset + ch.len_utf8();
            }
            return Some((&input[..end], end));
        }
    }
    if input.starts_with("\\\\?\\") {
        let end = input
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ')' | '(' | ',' | ';'))
            .unwrap_or(input.len());
        return Some((&input[..end], end));
    }
    None
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    format!(
        "{}…",
        input.chars().take(max_chars).collect::<String>()
    )
}

fn looks_like_low_level_tool_error(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        "error:",
        "traceback",
        "stack trace",
        "exit code",
        "exit status",
        "serde",
        "jsonrpc",
        "stderr",
        "ffprobe",
        "ffmpeg",
        "whisper-cli",
        "libmpv",
        "panic",
        "thread '",
        "os error",
        "command not found",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
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

/// Build permission response from user-selected option id.
pub fn permission_selected_result(option_id: &str) -> Value {
    json!({
        "outcome": { "outcome": "selected", "optionId": option_id }
    })
}

pub fn permission_cancelled_result() -> Value {
    json!({ "outcome": { "outcome": "cancelled" } })
}

pub fn extract_permission_options(params: &Value) -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    if let Some(arr) = params.get("options").and_then(Value::as_array) {
        for opt in arr {
            let option_id = opt
                .get("optionId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if option_id.is_empty() {
                continue;
            }
            let name = opt
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(option_id.as_str())
                .to_string();
            let kind = opt.get("kind").and_then(Value::as_str).map(str::to_string);
            out.push((option_id, name, kind));
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_initialize_roundtrip_shape() {
        let req = request(1, "initialize", initialize_params());
        let line = encode_line(&req).expect("encode");
        assert!(line.contains("initialize"));
        assert!(line.contains("lumina"));
        assert!(line.contains("readTextFile"));
        assert!(line.contains("writeTextFile"));
        assert!(line.contains("\"terminal\":true") || line.contains("\"terminal\": true"));
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
        assert_eq!(
            n.get("method").and_then(Value::as_str),
            Some("session/cancel")
        );
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

    #[test]
    fn session_new_requires_absolute_cwd() {
        let params = session_new_params(
            "D:/videos",
            crate::mcp::lumina_mcp_servers(std::path::Path::new(
                "D:/videos/.lumina/agent-context.json",
            )),
        );
        assert_eq!(params.get("cwd").and_then(Value::as_str), Some("D:/videos"));
        assert!(params.get("mcpServers").and_then(Value::as_array).is_some());
    }

    #[test]
    fn close_capability_object_counts_as_supported() {
        let value = json!({
            "result": {
                "protocolVersion": 1,
                "agentCapabilities": {
                    "sessionCapabilities": { "close": {} }
                }
            }
        });
        let init = parse_initialize_result(&value);
        assert!(init.supports_session_close);
    }

    #[test]
    fn pick_auth_prefers_chatgpt_when_config_present() {
        let init = InitializeResult {
            auth_methods: vec![
                AuthMethod {
                    id: "api-key".into(),
                    name: "API Key".into(),
                },
                AuthMethod {
                    id: "chat-gpt".into(),
                    name: "ChatGPT".into(),
                },
            ],
            ..InitializeResult::default()
        };
        let picked = pick_auth_method(&init).expect("method");
        assert_eq!(picked.id, "chat-gpt");
    }

    #[test]
    fn parses_session_model_and_reasoning_options() {
        let response = json!({
            "result": {
                "configOptions": [
                    { "id": "model", "currentValue": "mini", "options": [{ "value": "mini", "name": "Mini", "description": "Low cost" }] },
                    { "id": "reasoning_effort", "currentValue": "low", "options": [{ "value": "low", "name": "Low" }] }
                ]
            }
        });
        let options = parse_session_model_options(&response);
        assert_eq!(options.current_model_id.as_deref(), Some("mini"));
        assert_eq!(options.models[0].name, "Mini");
        assert_eq!(options.current_reasoning_effort.as_deref(), Some("low"));
    }

    #[test]
    fn extract_tool_call_content_and_sanitize_detail() {
        let value = json!({
            "method": "session/update",
            "params": {
                "update": {
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": "call-1",
                    "status": "failed",
                    "content": [
                        {
                            "type": "content",
                            "content": { "type": "text", "text": "无法读取当前播放上下文" }
                        }
                    ]
                }
            }
        });
        let tool = extract_tool_call(&value).expect("tool");
        assert_eq!(tool.status.as_deref(), Some("failed"));
        assert_eq!(
            tool.detail.as_deref(),
            Some("无法读取当前播放上下文")
        );
    }

    #[test]
    fn sanitize_tool_detail_redacts_low_level_errors() {
        let sanitized = sanitize_tool_detail("ffprobe: exit code 1 at D:\\movie\\a.mkv");
        assert_eq!(sanitized, "工具执行未成功，Agent 将尝试其他方式继续");
    }

    #[test]
    fn extract_tool_call_content_chunk() {
        let value = json!({
            "method": "session/update",
            "params": {
                "update": {
                    "sessionUpdate": "tool_call_content_chunk",
                    "toolCallId": "call-2",
                    "content": { "type": "text", "text": "步骤 1 完成" }
                }
            }
        });
        let chunk = super::extract_tool_call_content_chunk(&value).expect("chunk");
        assert_eq!(chunk.0, "call-2");
        assert_eq!(chunk.1, "步骤 1 完成");
    }
}
