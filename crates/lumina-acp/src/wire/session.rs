//! ACP session lifecycle wire helpers (initialize/session/* + initialize parsing).
//! Split from `wire/protocol.rs` without behavior change.

use std::path::Path;

use serde_json::{json, Value};

use crate::domain::context::VideoPromptContext;

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

/// `session/prompt` params: media travels as a `resource_link` only.
/// Structured playback data stays in the snapshot for MCP tools; stable tool
/// guidance comes from MCP `initialize.instructions`, never from here.
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
            // Online page URLs stay as-is (Agent fetches via MCP snapshot);
            // only local paths become file:// URIs. Never put cookies,
            // signed URLs, or cache paths here — snapshot is already sanitized.
            let uri = if is_remote_url(path) {
                path.to_string()
            } else {
                path_to_file_uri(path)
            };
            prompt.push(json!({
                "type": "resource_link",
                "uri": uri,
                "name": name,
            }));
        }

        if let Some(playback) = format_playback_context_block(ctx) {
            prompt.push(json!({
                "type": "text",
                "text": playback,
            }));
        }
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

/// Compact per-turn block: progress / episode index always;
/// episode title/overview only when enrich packed them on media switch.
fn format_playback_context_block(ctx: &VideoPromptContext) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(title) = ctx
        .media_title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            ctx.media_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
                .map(file_name)
        })
    {
        lines.push(format!("媒体：{title}"));
    }
    match (ctx.position_ms, ctx.duration_ms) {
        (Some(pos), Some(dur)) => lines.push(format!(
            "进度：{} / {}（{pos}ms）",
            format_clock_ms(pos),
            format_clock_ms(dur)
        )),
        (Some(pos), None) => lines.push(format!("进度：{}（{pos}ms）", format_clock_ms(pos))),
        (None, Some(dur)) => lines.push(format!("时长：{}（{dur}ms）", format_clock_ms(dur))),
        (None, None) => {}
    }
    match (ctx.season, ctx.episode) {
        (Some(season), Some(episode)) => lines.push(format!("集数：S{season:02}E{episode:02}")),
        (None, Some(episode)) => lines.push(format!("集数：E{episode:02}")),
        (Some(season), None) => lines.push(format!("季数：S{season:02}")),
        (None, None) => {}
    }
    if let Some(choice) = ctx
        .subtitle_choice_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        lines.push(format!("字幕轨道：{choice}"));
    }
    // Conditional: only present when enrich packed them on media switch.
    if let Some(title) = ctx
        .episode_title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        lines.push(format!("本集标题：{title}"));
    }
    if let Some(overview) = ctx
        .episode_overview
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        lines.push(format!("本集剧情：{overview}"));
    }
    if lines.is_empty() {
        return None;
    }
    Some(format!("【当前播放】\n{}", lines.join("\n")))
}

fn format_clock_ms(ms: u64) -> String {
    let total_sec = ms / 1000;
    let hours = total_sec / 3600;
    let minutes = (total_sec % 3600) / 60;
    let seconds = total_sec % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn is_remote_url(path: &str) -> bool {
    let lower = path.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn file_name(path: &str) -> &str {
    if is_remote_url(path) {
        return path;
    }
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

    use crate::domain::context::VideoPromptContext;

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
        let picked = crate::agent::launch::pick_auth_method(&init).expect("method");
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
    fn prompt_inlines_playback_progress_not_episode_plot_by_default() {
        let ctx = VideoPromptContext {
            media_path: Some(r"D:\videos\demo.mp4".into()),
            media_title: Some("demo.mp4".into()),
            position_ms: Some(83_000),
            duration_ms: Some(2_700_000),
            subtitle_choice_id: Some("embedded:0".into()),
            season: Some(1),
            episode: Some(1),
            episode_title: None,
            episode_overview: None,
        };
        let params = session_prompt_params("sess_1", "这段讲了什么？", Some(&ctx), None);
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        assert_eq!(prompt.len(), 3);
        let playback = prompt[1].get("text").and_then(Value::as_str).unwrap_or("");
        assert!(playback.contains("【当前播放】"));
        assert!(playback.contains("01:23"));
        assert!(playback.contains("S01E01"));
        assert!(!playback.contains("本集剧情"));
    }

    #[test]
    fn prompt_inlines_episode_plot_when_packed_for_media_switch() {
        let ctx = VideoPromptContext {
            media_path: Some(r"D:\videos\demo.mp4".into()),
            media_title: Some("demo.mp4".into()),
            position_ms: Some(83_000),
            duration_ms: Some(2_700_000),
            subtitle_choice_id: None,
            season: Some(1),
            episode: Some(2),
            episode_title: Some("第二集".into()),
            episode_overview: Some("换集剧情摘要".into()),
        };
        let params = session_prompt_params("sess_1", "讲了什么？", Some(&ctx), None);
        let playback = params
            .get("prompt")
            .and_then(Value::as_array)
            .and_then(|p| p.get(1))
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(playback.contains("本集标题：第二集"));
        assert!(playback.contains("本集剧情：换集剧情摘要"));
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
    fn online_page_url_stays_as_is_in_prompt() {
        let page = "https://www.youtube.com/watch?v=abc";
        let ctx = VideoPromptContext {
            media_path: Some(page.into()),
            media_title: Some("Demo".into()),
            position_ms: Some(10_000),
            duration_ms: Some(60_000),
            subtitle_choice_id: Some("online:en".into()),
            ..Default::default()
        };
        let params = session_prompt_params("sess_1", "讲了什么？", Some(&ctx), None);
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        let link = &prompt[0];
        assert_eq!(
            link.get("type").and_then(Value::as_str),
            Some("resource_link")
        );
        // Page URL preserved for MCP snapshot fetch; never rewritten to file://.
        assert_eq!(link.get("uri").and_then(Value::as_str), Some(page));
        assert_eq!(link.get("name").and_then(Value::as_str), Some("Demo"));
        let text = serde_json::to_string(&params)
            .expect("serialize")
            .to_lowercase();
        assert!(!text.contains("cookie"), "no cookie: {text}");
        assert!(!text.contains("sig="), "no signature: {text}");
        assert!(!text.contains("yt-dlp"), "no tool detail: {text}");
    }

    #[test]
    fn local_path_still_uses_file_uri() {
        let ctx = VideoPromptContext {
            media_path: Some(r"D:\videos\demo.mp4".into()),
            media_title: Some("demo.mp4".into()),
            position_ms: Some(1_000),
            duration_ms: None,
            subtitle_choice_id: None,
            ..Default::default()
        };
        let params = session_prompt_params("sess_1", "hi", Some(&ctx), None);
        let uri = params
            .get("prompt")
            .and_then(Value::as_array)
            .and_then(|p| p.first())
            .and_then(|l| l.get("uri"))
            .and_then(Value::as_str)
            .expect("uri");
        assert_eq!(uri, "file:///D:/videos/demo.mp4");
    }
}
