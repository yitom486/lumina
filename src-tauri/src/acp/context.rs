//! Video playback context attached to `session/prompt` (ACP content blocks).

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

        if let Some(summary) = build_context_summary(ctx) {
            prompt.push(json!({
                "type": "text",
                "text": summary,
            }));
        }
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

fn build_context_summary(ctx: &VideoPromptContext) -> Option<String> {
    let mut lines = vec!["【Lumina 视频上下文】".to_string()];

    if let Some(title) = ctx.media_title.as_deref().filter(|s| !s.trim().is_empty()) {
        lines.push(format!("媒体：{title}"));
    }
    if let Some(path) = ctx.media_path.as_deref().filter(|s| !s.trim().is_empty()) {
        lines.push(format!("路径：{path}"));
    }
    if ctx.position_ms.is_some() || ctx.duration_ms.is_some() {
        let pos = ctx
            .position_ms
            .map(format_time_ms)
            .unwrap_or_else(|| "?".into());
        let dur = ctx
            .duration_ms
            .map(format_time_ms)
            .unwrap_or_else(|| "?".into());
        lines.push(format!("进度：{pos} / {dur}"));
    }
    if let Some(ch) = ctx
        .chapter_title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        lines.push(format!("章节：{ch}"));
    }
    if let Some(excerpt) = ctx
        .transcript_excerpt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        lines.push("字幕摘录：".into());
        lines.push(excerpt.to_string());
    }
    if let Some(notes) = ctx
        .notes_excerpt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        lines.push("笔记摘录：".into());
        lines.push(notes.to_string());
    }

    lines.push("用户问题如下。".into());
    if lines.len() <= 2 {
        return None;
    }
    Some(lines.join("\n"))
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

fn path_to_file_uri(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
        format!("file:///{normalized}")
    } else if normalized.starts_with('/') {
        format!("file://{normalized}")
    } else {
        format!("file:///{normalized}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_includes_resource_link_and_user_text() {
        let ctx = VideoPromptContext {
            media_path: Some(r"D:\videos\demo.mp4".into()),
            media_title: Some("demo.mp4".into()),
            position_ms: Some(83_000),
            duration_ms: Some(2_700_000),
            chapter_title: Some("开场".into()),
            ..Default::default()
        };
        let params = session_prompt_params("sess_1", "这段讲了什么？", Some(&ctx));
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        assert_eq!(prompt.len(), 3);
        assert_eq!(
            prompt[0].get("type").and_then(Value::as_str),
            Some("resource_link")
        );
        assert!(prompt[0]
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("demo.mp4"));
        let summary = prompt[1].get("text").and_then(Value::as_str).unwrap_or("");
        assert!(summary.contains("Lumina"));
        assert!(summary.contains("开场"));
        assert_eq!(
            prompt[2].get("text").and_then(Value::as_str),
            Some("这段讲了什么？")
        );
    }

    #[test]
    fn prompt_without_context_is_user_text_only() {
        let params = session_prompt_params("sess_1", "你好", None);
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
}
