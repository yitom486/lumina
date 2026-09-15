//! ACP `session/update` extraction helpers.
//! Split from `wire/protocol.rs` without behavior change.

use serde_json::Value;

use super::sanitize::sanitize_tool_detail;

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

fn tool_call_content_item_text(item: &Value) -> Option<String> {
    match item.get("type").and_then(Value::as_str) {
        Some("content") => content_blocks_text(item.get("content")),
        Some("text") => item.get("text").and_then(Value::as_str).map(str::to_string),
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
        assert_eq!(tool.detail.as_deref(), Some("无法读取当前播放上下文"));
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
