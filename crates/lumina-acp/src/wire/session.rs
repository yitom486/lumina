//! ACP session lifecycle wire helpers (initialize/session/* + initialize parsing).
//! Split from `wire/protocol.rs` without behavior change.

use std::path::Path;

use serde_json::{json, Value};

use crate::domain::context::VideoPromptContext;
use crate::domain::model::{AgentSessionInfo, ResumeOutcome, SessionKind};

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
pub fn session_new_params(cwd: &str, mcp_servers: Value, kind: SessionKind) -> Value {
    json!({
        "cwd": cwd,
        "mcpServers": mcp_servers,
        "_meta": {
            "lumina": {
                "kind": kind.as_str(),
            },
        },
    })
}

/// `session/prompt` params: media travels as a `resource_link` only.
/// Structured playback data stays in the snapshot for MCP tools; stable tool
/// guidance lives in MCP `initialize.instructions`. The one exception is the
/// fixed trigger header below: it rides with the question (no timing
/// dependency on MCP readiness) and names only the frozen tool contract —
/// full rules stay in `initialize.instructions`.
const TOOL_TRIGGER_HEADER: &str = "【工具优先】本轮优先使用 Lumina 本地工具（实际可用以 tools/list 返回为准）：lumina_get_playback_context（播放锚点）、lumina_get_library_context（剧集简介）、lumina_get_episode_index（分集列表）、lumina_get_transcript_window（当前台词）、lumina_get_episode_transcript（他集台词）、lumina_get_audio_marks（音频信号）、lumina_capture_frames（视频截帧）、lumina_propose_video_annotation（批注提议）。剧情类问题禁止先网络搜索；若 tools/list 暂无 lumina 工具，说明接入未完成，请直接说明。";
const USER_PROMPT_SEPARATOR: &str = "\n\n";

pub fn session_prompt_params(
    session_id: &str,
    text: &str,
    context: Option<&VideoPromptContext>,
) -> Value {
    let mut prompt = Vec::new();
    let has_turn_context = context.is_some_and(|ctx| !ctx.is_empty());

    if let Some(ctx) = context.filter(|c| !c.is_empty()) {
        prompt.push(json!({
            "type": "text",
            "text": TOOL_TRIGGER_HEADER,
        }));
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

    prompt.push(json!({
        "type": "text",
        "text": if has_turn_context {
            format!("{USER_PROMPT_SEPARATOR}{text}")
        } else {
            text.to_string()
        },
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

/// `session/load` params: thread history comes back as streamed
/// `session/update` notifications, not in the final result (codex-acp
/// `loadSession` → `getOrCreateSessionWithHistory` + `streamThreadHistory`).
/// Shape mirrors `session/resume` (`zLoadSessionRequest` requires
/// `sessionId` + `cwd` + `mcpServers`).
pub fn session_load_params(session_id: &str, cwd: &str, mcp_servers: Value) -> Value {
    json!({
        "sessionId": session_id,
        "cwd": cwd,
        "mcpServers": mcp_servers,
    })
}

/// Tell an occupied conversation apart from a lost one.
///
/// Codex enforces a single writer per conversation and refuses to reattach one
/// that another process still holds, reporting `thread <id> already has an
/// active writer`. The stored conversation is untouched in that case, so it
/// must not be reported as lost. Matching agent wording is admittedly a weak
/// signal, but it is the only one the protocol exposes, and anything
/// unrecognised falls back to the conservative `Unavailable`.
pub fn classify_resume_failure(details: Option<&str>) -> ResumeOutcome {
    let occupied = details
        .map(|value| value.to_ascii_lowercase().contains("active writer"))
        .unwrap_or(false);
    if occupied {
        ResumeOutcome::Occupied
    } else {
        ResumeOutcome::Unavailable
    }
}

pub fn session_list_params(cwd: Option<&str>, cursor: Option<&str>) -> Value {
    let mut params = json!({});
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        params["cwd"] = json!(cwd);
    }
    if let Some(cursor) = cursor.filter(|value| !value.trim().is_empty()) {
        params["cursor"] = json!(cursor);
    }
    params
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
    pub supports_session_list: bool,
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

    let supports_session_list =
        capability_present(result, "/agentCapabilities/sessionCapabilities/list");

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
        supports_session_list,
        load_session,
    }
}

pub fn parse_session_list(value: &Value) -> (Vec<AgentSessionInfo>, Option<String>) {
    let result = value.get("result").unwrap_or(value);
    let mut sessions = Vec::new();
    if let Some(items) = result.get("sessions").and_then(Value::as_array) {
        for item in items {
            let Some(session_id) = item
                .get("sessionId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            else {
                continue;
            };
            let Some(cwd) = item
                .get("cwd")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            else {
                continue;
            };
            // Existing Agent sessions predate this metadata and therefore
            // remain visible; kind is recorded only for future filtering.
            let kind = item
                .get("_meta")
                .and_then(|meta| meta.get("lumina"))
                .and_then(|lumina| lumina.get("kind"))
                .and_then(Value::as_str)
                .map(str::to_string);
            sessions.push(AgentSessionInfo {
                session_id: session_id.to_string(),
                cwd: cwd.to_string(),
                title: item
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                updated_at: item
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                kind,
            });
        }
    }
    let next_cursor = result
        .get("nextCursor")
        .and_then(Value::as_str)
        .map(str::to_string);
    (sessions, next_cursor)
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
    fn session_load_params_mirror_resume_shape() {
        // Shape pinned to codex-acp `zLoadSessionRequest`
        // (`sessionId` + `cwd` + `mcpServers`); history itself arrives as
        // streamed `session/update`, never in the final result.
        let mcp_servers = json!([]);
        let params = session_load_params("sess-1", "D:/videos", mcp_servers.clone());
        assert_eq!(
            params,
            json!({
                "sessionId": "sess-1",
                "cwd": "D:/videos",
                "mcpServers": [],
            })
        );
        assert_eq!(
            params,
            session_resume_params("sess-1", "D:/videos", mcp_servers)
        );
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
            SessionKind::Chat,
        );
        assert_eq!(params.get("cwd").and_then(Value::as_str), Some("D:/videos"));
        assert!(params.get("mcpServers").and_then(Value::as_array).is_some());
        assert_eq!(
            params.pointer("/_meta/lumina/kind").and_then(Value::as_str),
            Some("chat")
        );
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
    fn occupied_resume_failure_is_not_reported_as_lost() {
        // Shape taken from a real codex-acp refusal.
        assert_eq!(
            classify_resume_failure(Some(
                "Internal error: thread 01a09b84-d8a8-7462-aade-7c4c1da433e2 \
                 already has an active writer"
            )),
            ResumeOutcome::Occupied
        );
        assert_eq!(
            classify_resume_failure(Some("Thread ABC Already Has An Active Writer")),
            ResumeOutcome::Occupied
        );
    }

    #[test]
    fn unrecognized_resume_failure_falls_back_to_unavailable() {
        for details in [
            None,
            Some("Internal error"),
            Some("no rollout found for thread id abc"),
            Some(""),
        ] {
            assert_eq!(
                classify_resume_failure(details),
                ResumeOutcome::Unavailable,
                "details={details:?}"
            );
        }
    }

    #[test]
    fn session_list_capability_accepts_object_and_true_but_not_missing() {
        for capability in [json!({}), json!(true)] {
            let value = json!({
                "result": {
                    "agentCapabilities": {
                        "sessionCapabilities": { "list": capability }
                    }
                }
            });
            assert!(parse_initialize_result(&value).supports_session_list);
        }
        let missing = json!({
            "result": {
                "agentCapabilities": { "sessionCapabilities": {} }
            }
        });
        assert!(!parse_initialize_result(&missing).supports_session_list);
    }

    #[test]
    fn parses_session_list_and_skips_invalid_items() {
        let response = json!({
            "result": {
                "sessions": [
                    {
                        "sessionId": "s1",
                        "cwd": "D:/movie",
                        "title": "看剧对话",
                        "updatedAt": "2026-09-16T10:00:00Z",
                        "_meta": { "lumina": { "kind": "chat" }, "private": true },
                        "additionalDirectories": ["D:/other"]
                    },
                    { "cwd": "D:/movie", "title": "缺 id" },
                    "not-an-object",
                    {
                        "sessionId": "s2",
                        "cwd": "D:/movie",
                        "title": null,
                        "updatedAt": null,
                        "_meta": { "lumina": { "kind": { "unexpected": true } } }
                    }
                ],
                "nextCursor": "cursor-2"
            }
        });
        let (sessions, cursor) = parse_session_list(&response);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "s1");
        assert_eq!(sessions[0].title.as_deref(), Some("看剧对话"));
        assert_eq!(sessions[0].kind.as_deref(), Some("chat"));
        assert_eq!(sessions[1].session_id, "s2");
        assert_eq!(sessions[1].title, None);
        assert_eq!(sessions[1].kind, None);
        assert_eq!(cursor.as_deref(), Some("cursor-2"));
    }

    #[test]
    fn session_list_parser_handles_empty_array_and_params() {
        let (sessions, cursor) = parse_session_list(&json!({
            "result": { "sessions": [] }
        }));
        assert!(sessions.is_empty());
        assert_eq!(cursor, None);
        assert_eq!(
            session_list_params(Some("D:/movie"), Some("cursor-1")),
            json!({ "cwd": "D:/movie", "cursor": "cursor-1" })
        );
        assert_eq!(session_list_params(None, None), json!({}));
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
        let params = session_prompt_params("sess_1", "这段讲了什么？", Some(&ctx));
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        assert_eq!(prompt.len(), 4);
        let header = prompt[0].get("text").and_then(Value::as_str).unwrap_or("");
        assert!(header.contains("【工具优先】"));
        assert!(header.contains("lumina_get_transcript_window"));
        assert!(header.contains("禁止先网络搜索"));
        let playback = prompt[2].get("text").and_then(Value::as_str).unwrap_or("");
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
        let params = session_prompt_params("sess_1", "讲了什么？", Some(&ctx));
        let playback = params
            .get("prompt")
            .and_then(Value::as_array)
            .and_then(|p| p.get(2))
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(playback.contains("本集标题：第二集"));
        assert!(playback.contains("本集剧情：换集剧情摘要"));
    }

    #[test]
    fn tool_trigger_header_leads_when_context_present() {
        let ctx = VideoPromptContext {
            media_path: Some(r"D:\videos\demo.mp4".into()),
            position_ms: Some(5_000),
            ..Default::default()
        };
        let params = session_prompt_params("sess_1", "讲了什么？", Some(&ctx));
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        // Header first, user text last; resource link and playback keep order.
        let first = prompt
            .first()
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(first.starts_with("【工具优先】"));
        // All eight Chat tools named with one-phrase intros; workshop-only
        // write tools stay out of the per-turn header.
        for name in [
            "lumina_get_playback_context",
            "lumina_get_library_context",
            "lumina_get_episode_index",
            "lumina_get_transcript_window",
            "lumina_get_episode_transcript",
            "lumina_get_audio_marks",
            "lumina_capture_frames",
            "lumina_propose_video_annotation",
        ] {
            assert!(first.contains(name), "header must name {name}");
        }
        let last = prompt
            .last()
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert_eq!(last, "\n\n讲了什么？");
        let serialized = serde_json::to_string(&params).expect("serialize");
        assert!(
            !serialized.contains("mediaPath"),
            "no camelCase internals: {serialized}"
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
    fn prompt_never_includes_history_summary() {
        let params = session_prompt_params("sess_1", "继续问", None);
        let serialized = serde_json::to_string(&params).expect("serialize");
        let removed_summary_marker = ["此前", "对话摘要"].concat();
        assert!(!serialized.contains(removed_summary_marker.as_str()));
        assert!(serialized.contains("继续问"));
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
        let params = session_prompt_params("sess_1", "讲了什么？", Some(&ctx));
        let prompt = params
            .get("prompt")
            .and_then(Value::as_array)
            .expect("prompt");
        let link = &prompt[1];
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
        let params = session_prompt_params("sess_1", "hi", Some(&ctx));
        let uri = params
            .get("prompt")
            .and_then(Value::as_array)
            .and_then(|p| p.get(1))
            .and_then(|l| l.get("uri"))
            .and_then(Value::as_str)
            .expect("uri");
        assert_eq!(uri, "file:///D:/videos/demo.mp4");
    }
}
