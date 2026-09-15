//! ACP session lifecycle wire helpers (initialize/session/* + initialize parsing).
//! Split from `wire/protocol.rs` without behavior change.

use serde_json::{json, Value};

use crate::agent::discover::codex_config_present;

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

/// Deprecated: only kept for the old `crate::wire::protocol::session_prompt_params` path.
/// New code should call `crate::domain::context::session_prompt_params` directly.
pub fn session_prompt_params(session_id: &str, text: &str) -> Value {
    crate::domain::context::session_prompt_params(session_id, text, None, None)
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

pub fn parse_session_model_options(value: &Value) -> crate::domain::model::AcpSessionModelOptions {
    let result = value.get("result").unwrap_or(value);
    let mut options = crate::domain::model::AcpSessionModelOptions::default();
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
                (!value.is_empty()).then(|| crate::domain::model::AcpSessionOption {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::updates::extract_agent_text;
    use super::*;

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
    fn session_new_requires_absolute_cwd() {
        let params = session_new_params(
            "D:/videos",
            // Generalized MCP server spec (shape mirrors `mcp::lumina_mcp_servers`).
            serde_json::json!([{
                "name": "lumina",
                "command": "lumina",
                "args": ["--lumina-mcp"],
                "env": [{
                    "name": "LUMINA_MCP_CONTEXT_FILE",
                    "value": "D:/videos/.lumina/agent-context.json",
                }],
            }]),
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
}
