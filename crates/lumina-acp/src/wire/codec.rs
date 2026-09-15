//! ACP JSON-RPC codec: request/notification/response helpers + inbound classification.
//! Split from `wire/protocol.rs` without behavior change.

use serde_json::{json, Value};

use crate::error::AcpError;

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

pub fn encode_line(value: &Value) -> Result<String, AcpError> {
    serde_json::to_string(value).map_err(|error| {
        tracing::warn!(%error, "failed to serialize ACP JSON");
        AcpError::protocol(Some(&format!("serialize ACP JSON: {error}")))
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::session::{initialize_params, session_cancel_params};
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
}
