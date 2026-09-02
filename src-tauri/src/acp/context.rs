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
    pub subtitle_choice_id: Option<String>,
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
                .subtitle_choice_id
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
    history_context: Option<&str>,
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

        prompt.push(json!({
            "type": "text",
            "text": context_pointer_text(),
        }));
    }

    if let Some(history) = history_context.filter(|s| !s.trim().is_empty()) {
        prompt.push(json!({
            "type": "text",
            "text": format!("【此前对话摘要】\n{history}"),
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
    "【Lumina】用户正在本机观看上述媒体。可用 MCP 工具：\
lumina_get_playback_context、lumina_get_library_context、lumina_get_episode_index、\
lumina_get_transcript_window、lumina_get_episode_transcript、lumina_propose_video_annotation（识图模型另有 lumina_capture_frames）。\
\n\n工具调用原则：\
(1) 若当前对话、此前工具结果或问题本身已足够回答，直接作答，不要重复调用。\
(2) 仅缺哪类信息再增量调用对应工具，避免每轮并行全量拉取。\
(3) 未通过工具确认的剧情/角色/台词不要编造；上下文不足时，先调用工具补齐必要信息再回答。\
(4) 时间基准：本回合向 Agent 提供的播放进度、章节、附近笔记与 MCP 工具均共用「提问锚点」anchor.positionMs（用户在本输入框**开始键入**时冻结，非实时；连续输入间隔不超过 10 秒则沿用同一锚点，超过 10 秒无输入后再次键入则重新锚定）。字幕/截图默认以此为中心，可用 centerMs/atSec 覆盖；跨集字幕在目标集与锚点同集时亦默认锚点，否则默认该集起点。\
问当前集剧情、对话、人物关系：优先 lumina_get_transcript_window（配合 beforeSec/afterSec 扩大窗口）；\
问其它集台词用 lumina_get_episode_transcript（season/episode 必填，可选 centerMs/atSec）；\
单帧截图看不清台词或需要画面/场景/表情细节时，再用 lumina_capture_frames。\
(5) 分集列表与媒体库背景分别用 lumina_get_episode_index、lumina_get_library_context。\
(6) 写视频批注：先调用 lumina_propose_video_annotation 生成提议（含正文与引用台词预览），**禁止**直接写入笔记库；用户会在该条回复下方的确认卡片中保存或取消。保存成功后界面会显示「批注已写入笔记库」，无需反复提醒用户去别处确认。\
制作/翻译外挂字幕请使用文稿面板的「翻译字幕」或 ASR，不要在本对话中尝试写入字幕轨。"
}

#[cfg(test)]
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
            subtitle_choice_id: Some("embedded:0".into()),
            notes_excerpt: Some("也不应出现".into()),
        };
        let params = session_prompt_params("sess_1", "这段讲了什么？", Some(&ctx), None);
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        assert_eq!(prompt.len(), 3);
        assert_eq!(
            prompt[0].get("type").and_then(Value::as_str),
            Some("resource_link")
        );
        let pointer = prompt[1].get("text").and_then(Value::as_str).unwrap_or("");
        assert!(pointer.contains("MCP 工具"));
        assert!(pointer.contains("直接作答"));
        assert!(pointer.contains("增量调用"));
        assert!(pointer.contains("全量拉取"));
        assert!(pointer.contains("lumina_get_transcript_window"));
        assert!(pointer.contains("lumina_get_episode_transcript"));
        assert!(pointer.contains("lumina_propose_video_annotation"));
        assert!(pointer.contains("文稿面板"));
        assert!(!pointer.contains("lumina_write_subtitle_track"));
        assert!(!pointer.contains("lumina_get_subtitle_cues"));
        assert!(pointer.contains("lumina_capture_frames"));
        assert!(pointer.contains("先调用工具补齐"));
        assert!(!pointer.contains("也不应出现"));
        assert_eq!(
            prompt[2].get("text").and_then(Value::as_str),
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
    fn prompt_includes_history_context_before_user_text() {
        let params = session_prompt_params(
            "sess_1",
            "继续问",
            None,
            Some("用户：你好\n\n助手：你好，有什么可以帮你？"),
        );
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        assert_eq!(prompt.len(), 2);
        let history = prompt[0].get("text").and_then(Value::as_str).unwrap_or("");
        assert!(history.contains("此前对话摘要"));
        assert!(history.contains("用户：你好"));
        assert_eq!(
            prompt[1].get("text").and_then(Value::as_str),
            Some("继续问")
        );
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
