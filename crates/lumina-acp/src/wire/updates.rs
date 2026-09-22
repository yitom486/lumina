//! ACP `session/update` extraction helpers (official SDK types; tolerant fallback kept).
//!
//! Primary path parses `agent_client_protocol::schema::v1::SessionUpdate` so
//! update shapes track the spec instead of hand-written string matching.
//! Two deliberate manual layers remain, both documented at the call site:
//! - scalar display strings (`title`/`status`/`kind`) come from the raw JSON
//!   with the historical presence semantics (missing stays missing instead of
//!   becoming an SDK default), because they are UI text, not protocol;
//! - updates the SDK cannot parse (unknown future variants, codex
//!   `tool_call_content_chunk` extension) fall back to the historical walkers.

use agent_client_protocol::schema::v1::{ContentBlock, SessionUpdate, ToolCallContent};
use serde_json::Value;

use super::sanitize::sanitize_tool_detail;

/// Extract assistant-visible text from `session/update` agent_message_chunk.
pub fn extract_agent_text(value: &Value) -> Option<String> {
    let update = session_update_payload(value)?;
    if let Ok(SessionUpdate::AgentMessageChunk(chunk)) =
        serde_json::from_value::<SessionUpdate>(update.clone())
    {
        if let Some(text) = content_block_text(&chunk.content) {
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    if update.get("sessionUpdate").and_then(Value::as_str) == Some("agent_message_chunk") {
        return content_blocks_text_manual(update.get("content")).filter(|text| !text.is_empty());
    }
    None
}

pub fn extract_thought_text(value: &Value) -> Option<String> {
    let update = session_update_payload(value)?;
    if let Ok(SessionUpdate::AgentThoughtChunk(chunk)) =
        serde_json::from_value::<SessionUpdate>(update.clone())
    {
        if let Some(text) = content_block_text(&chunk.content) {
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    if update.get("sessionUpdate").and_then(Value::as_str) == Some("agent_thought_chunk") {
        return content_blocks_text_manual(update.get("content")).filter(|text| !text.is_empty());
    }
    None
}

pub fn extract_tool_call(value: &Value) -> Option<ToolCallInfo> {
    let update = session_update_payload(value)?;
    let parsed = serde_json::from_value::<SessionUpdate>(update.clone()).ok()?;
    match parsed {
        SessionUpdate::ToolCall(call) => Some(tool_call_info(
            "tool_call",
            update,
            &call.tool_call_id.to_string(),
            &call.content,
            call.raw_output.as_ref(),
        )),
        SessionUpdate::ToolCallUpdate(call) => Some(tool_call_info(
            "tool_call_update",
            update,
            &call.tool_call_id.to_string(),
            &call.fields.content.clone().unwrap_or_default(),
            call.fields.raw_output.as_ref(),
        )),
        // 未知变体（含未来扩展）：不认，不断连接。
        _ => None,
    }
}

/// `tool_call_content_chunk` is a codex extension, not a spec variant:
/// kept fully manual by design (SDK has no such update kind).
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

fn tool_call_info(
    update_kind: &str,
    raw_update: &Value,
    tool_call_id: &str,
    content: &[ToolCallContent],
    raw_output: Option<&Value>,
) -> ToolCallInfo {
    ToolCallInfo {
        update_kind: update_kind.to_string(),
        // Display scalars stay raw: missing stays missing (never an SDK default).
        tool_call_id: raw_update
            .get("toolCallId")
            .and_then(Value::as_str)
            .unwrap_or(tool_call_id)
            .to_string(),
        title: raw_update
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        status: raw_update
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string),
        kind: raw_update
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        detail: tool_call_detail(content, raw_update, raw_output),
        append_detail: false,
    }
}

fn tool_call_detail(
    content: &[ToolCallContent],
    raw_update: &Value,
    raw_output: Option<&Value>,
) -> Option<String> {
    let mut parts = Vec::new();
    for item in content {
        let text = match item {
            ToolCallContent::Content(wrapper) => content_block_text(&wrapper.content),
            // Diff/Terminal carry no display text in spec; typeless extras
            // are covered by the manual fallback below.
            _ => None,
        };
        if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
            parts.push(text);
        }
    }
    if !parts.is_empty() {
        return Some(parts.join("\n"));
    }
    // Manual leniency: typeless text fields and legacy shapes.
    if let Some(text) = extract_tool_call_content_text_manual(raw_update) {
        let sanitized = sanitize_tool_detail(&text);
        if !sanitized.is_empty() {
            return Some(sanitized);
        }
    }
    raw_output
        .and_then(value_to_plain_text)
        .map(|text| sanitize_tool_detail(&text))
        .filter(|text| !text.is_empty())
        .or_else(|| {
            raw_update
                .get("rawOutput")
                .and_then(value_to_plain_text)
                .map(|text| sanitize_tool_detail(&text))
                .filter(|text| !text.is_empty())
        })
}

pub fn extract_plan_summary(value: &Value) -> Option<String> {
    let update = session_update_payload(value)?;
    // Structural gate on the SDK type; entry formatting stays raw because
    // status strings are UI text ("pending" default preserved verbatim).
    if serde_json::from_value::<SessionUpdate>(update.clone()).is_err() {
        return extract_plan_summary_manual(update);
    }
    extract_plan_summary_manual(update)
}

fn content_block_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(text.text.clone()),
        // Other variants (image/audio/resource/diff carriers) have no
        // display text in spec; typeless extras use the manual walker.
        _ => None,
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

// --- Manual fallback walkers (historical leniency, kept minimal) ---

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
    extract_tool_call_content_text_manual(update)
}

fn extract_tool_call_content_text_manual(update: &Value) -> Option<String> {
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
    content_blocks_text_manual(Some(content))
}

fn tool_call_content_item_text(item: &Value) -> Option<String> {
    match item.get("type").and_then(Value::as_str) {
        Some("content") => content_blocks_text_manual(item.get("content")),
        Some("text") => item.get("text").and_then(Value::as_str).map(str::to_string),
        _ => content_blocks_text_manual(Some(item)),
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

fn content_blocks_text_manual(content: Option<&Value>) -> Option<String> {
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

fn extract_plan_summary_manual(update: &Value) -> Option<String> {
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

    #[test]
    fn unknown_update_kinds_are_ignored_not_fatal() {
        // 未来变体：SDK 整单拒绝，走手工兜底同样认不出 → None，不断连接。
        let value = json!({
            "method": "session/update",
            "params": {
                "update": { "sessionUpdate": "frobnicator_v9", "content": "x" }
            }
        });
        assert_eq!(extract_agent_text(&value), None);
        assert!(extract_tool_call(&value).is_none());
        assert_eq!(extract_plan_summary(&value), None);
    }
}
