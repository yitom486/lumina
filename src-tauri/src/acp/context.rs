//! Video playback context attached to `session/prompt` (ACP content blocks).
//!
//! Structured metadata is **not** inlined into the prompt. Lumina writes
//! `.lumina/agent-context.json` and registers an MCP server so the Agent can
//! fetch playback/library context on demand.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::mcp::SNAPSHOT_RELATIVE_PATH;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VideoPromptContext {
    pub media_path: Option<String>,
    pub media_title: Option<String>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub chapter_title: Option<String>,
    pub transcript_excerpt: Option<String>,
    pub notes_excerpt: Option<String>,
}

impl VideoPromptContext {
    pub fn is_empty(&self) -> bool {
        self.media_path.as_ref().is_none_or(|s| s.trim().is_empty())
            && self
                .media_title
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            && self.position_ms.is_none()
            && self.duration_ms.is_none()
            && self
                .chapter_title
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            && self
                .transcript_excerpt
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            && self
                .notes_excerpt
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
    }
}

pub fn session_prompt_params(
    session_id: &str,
    text: &str,
    context: Option<&VideoPromptContext>,
    context_snapshot: Option<&Path>,
) -> Value {
    let mut prompt = Vec::new();

    if let Some(ctx) = context.filter(|c| !c.is_empty()) {
        if let Some(path) = ctx.media_path.as_deref().filter(|p| !p.trim().is_empty()) {
            let name = ctx
                .media_title
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| file_name(path));
            prompt.push(json!({
                "type": "resource_link",
                "uri": path_to_file_uri(path),
                "name": name,
            }));
        }

        if let Some(snapshot) = context_snapshot {
            prompt.push(json!({
                "type": "resource_link",
                "uri": path_to_file_uri(&snapshot.to_string_lossy()),
                "name": format!("Lumina 媒体上下文 ({SNAPSHOT_RELATIVE_PATH})"),
            }));
        }

        prompt.push(json!({
            "type": "text",
            "text": context_pointer_text(),
        }));
    }

    prompt.push(json!({
        "type": "text",
        "text": text,
    }));

    json!({
        "sessionId": session_id,
        "prompt": prompt,
    })
}

fn context_pointer_text() -> &'static str {
    "【Lumina】用户正在本机观看媒体。播放进度、字幕摘录、笔记，以及 TMDb/维基百科合并元数据已写入会话目录下的 `.lumina/agent-context.json`。\
请优先调用 MCP 工具 `lumina_get_playback_context` 与 `lumina_get_library_context` 按需读取；\
若 MCP 不可用，可读取上述 JSON 文件。不要臆造未读取到的剧情或角色信息。"
}

fn format_time_ms(ms: u64) -> String {
    let total_sec = ms / 1000;
    let h = total_sec / 3600;
    let m = (total_sec % 3600) / 60;
    let s = total_sec % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

pub fn path_to_file_uri(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
        format!("file:///{normalized}")
    } else if normalized.starts_with('/') {
        format!("file://{normalized}")
    } else {
        format!("file:///{normalized}")
    }
}

pub fn snapshot_display_path(cwd: &Path) -> PathBuf {
    cwd.join(SNAPSHOT_RELATIVE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_uses_resource_links_not_inline_metadata() {
        let ctx = VideoPromptContext {
            media_path: Some(r"D:\videos\demo.mp4".into()),
            media_title: Some("demo.mp4".into()),
            position_ms: Some(83_000),
            duration_ms: Some(2_700_000),
            chapter_title: Some("开场".into()),
            transcript_excerpt: Some("不应出现在 prompt 中".into()),
            notes_excerpt: Some("也不应出现".into()),
        };
        let snapshot = PathBuf::from(r"D:\workspace\.lumina\agent-context.json");
        let params = session_prompt_params("sess_1", "这段讲了什么？", Some(&ctx), Some(&snapshot));
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        assert_eq!(prompt.len(), 4);
        assert_eq!(
            prompt[0].get("type").and_then(Value::as_str),
            Some("resource_link")
        );
        assert_eq!(
            prompt[1].get("type").and_then(Value::as_str),
            Some("resource_link")
        );
        let pointer = prompt[2].get("text").and_then(Value::as_str).unwrap_or("");
        assert!(pointer.contains("lumina_get_playback_context"));
        assert!(!pointer.contains("不应出现在 prompt 中"));
        assert_eq!(
            prompt[3].get("text").and_then(Value::as_str),
            Some("这段讲了什么？")
        );
    }

    #[test]
    fn prompt_without_context_is_user_text_only() {
        let params = session_prompt_params("sess_1", "你好", None, None);
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        assert_eq!(prompt.len(), 1);
        assert_eq!(prompt[0].get("text").and_then(Value::as_str), Some("你好"));
    }

    #[test]
    fn windows_path_to_file_uri() {
        assert_eq!(
            path_to_file_uri(r"D:\videos\a.mp4"),
            "file:///D:/videos/a.mp4"
        );
    }

    #[test]
    fn format_time_ms_helpers() {
        assert_eq!(format_time_ms(83_000), "1:23");
    }
}
