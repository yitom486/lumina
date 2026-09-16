//! Minimal MCP stdio server exposing Lumina context tools.

use std::io::{self, BufRead, BufWriter, Write};
use std::sync::{Arc, Condvar, Mutex, RwLock};

use serde_json::{json, Value};

use super::executor::TaskExecutor;
use super::policy::{tool_profile_from_env, McpToolProfile, ToolPolicy};
use super::snapshot::{read_snapshot, resolve_snapshot_path, LuminaMcpSnapshot};
use super::tools::handle_tool_call;

const PROTOCOL_VERSION: &str = "2024-11-05";
const TOOL_WORKER_COUNT: usize = 4;
const HEAVY_TOOL_CONCURRENCY: usize = 2;

type Output = Arc<Mutex<BufWriter<io::Stdout>>>;
type ToolState = Arc<RwLock<()>>;

// MCP InitializeResult.instructions is the stable, session-level place for
// guidance that the host may append to the model's system context. Keep this
// free of turn-specific values: playback state and library data live in the
// snapshot and are fetched through tools when needed.
//
// The catalog below is eager: the model learns all tool identities here,
// before/without waiting for tools/list. tools/list still owns the callable
// subset and JSON schemas for this session.
const MCP_SERVER_INSTRUCTIONS: &str = "Lumina 提供与当前媒体相关的按需上下文工具；完整目录共 10 个，初始化时即应拉取 tools/list，不要靠猜。实际可调用子集与参数 schema 以 tools/list 返回为准。\n\n目录：\n- lumina_get_playback_context：当前锚点与本集信息。\n- lumina_get_library_context：剧集背景与当前集简介。\n- lumina_get_episode_index：全部分集标题与简介。\n- lumina_get_transcript_window：当前文件锚点附近台词；可选 centerMs/atSec，beforeSec/afterSec/radiusSec 默认前后各60秒。\n- lumina_get_episode_transcript：同剧其他集台词；必填 season/episode。\n- lumina_get_audio_marks：静音区间与响度突增候选（非语义标签）。\n- lumina_get_subtitle_cues / lumina_write_subtitle_track：字幕工坊专用。\n- lumina_capture_frames：锚点附近截帧；仅识图会话可见。\n- lumina_propose_video_annotation：视频批注提议；由用户在界面确认。\n\n工具调用原则：\n(1) 如果当前对话、此前工具结果或问题本身已经足够回答，直接作答，不要重复调用。\n(2) 只调用当前缺失的信息对应的工具，避免每轮并行全量拉取。\n(3) 台词原文和具体剧情点以工具返回为准，禁止先走网络搜索或编造；基于已验证内容的解读、动机分析和前后联系可以直接展开。\n(4) 问本集讲了什么、剧情或对话，必须先调 lumina_get_library_context 或 lumina_get_transcript_window；问其他集必须先调 lumina_get_episode_transcript；问画面细节必须先调 lumina_capture_frames；播放锚点、章节、笔记和字幕/截图工具共用本轮冻结的锚点位置。\n(5) 分集列表使用 lumina_get_episode_index，剧集背景和当前集简介使用 lumina_get_library_context。\n(6) 写视频批注必须先调用 lumina_propose_video_annotation 生成提议，禁止直接写入笔记库；由用户在 Lumina 界面确认保存。\n(7) 引用视频内容使用工具实际返回的时间标记，例如 [03:12]；跨集引用使用 [第N集 · mm:ss]，不要编造时间。\n(8) 跨集引用默认只使用当前集及之前的集数；用户明确要求后续集数时才查询，并提示剧透。\n(9) 制作或翻译外挂字幕请使用 Lumina 文稿面板或 ASR 工作流，不要在本对话中尝试写入字幕轨。\n(10) 若目录中的工具不在 tools/list 中，视为本会话未开放：不要手写调用、不要猜测其返回；如用户追问画面细节而无截图工具，应明说本会话不支持画面分析并基于字幕作答。";

/// Timing probe for the serial-vs-parallel question. stderr only: stdout
/// must stay pure JSON-RPC, and this process exits before the app's tracing
/// subscriber exists, so `tracing!` would be a no-op here.
fn log_timing(event: &str, id: &Value, tool: &str, elapsed_ms: Option<u128>) {
    match elapsed_ms {
        Some(elapsed_ms) => {
            eprintln!("[lumina-mcp-timing] {event} id={id} tool={tool} elapsed_ms={elapsed_ms}")
        }
        None => eprintln!("[lumina-mcp-timing] {event} id={id} tool={tool}"),
    }
}

pub fn run_stdio_server() -> Result<(), String> {
    let stdin = io::stdin();
    let output: Output = Arc::new(Mutex::new(BufWriter::new(io::stdout())));
    let state: ToolState = Arc::new(RwLock::new(()));
    let heavy_limiter = Arc::new(HeavyToolLimiter::new(HEAVY_TOOL_CONCURRENCY));
    let executor = TaskExecutor::new(TOOL_WORKER_COUNT);

    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("stdin read: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value =
            serde_json::from_str(&line).map_err(|error| format!("invalid json: {error}"))?;
        if request.get("method").and_then(Value::as_str) == Some("notifications/initialized") {
            continue;
        }
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "initialize" => write_response(&output, success(id, initialize_result(params)))?,
            "tools/list" => {
                let profile = tool_profile_from_env();
                // Tool-free tasks never load the Chat snapshot file.
                if profile == McpToolProfile::NoTools {
                    write_response(&output, success(id, json!({ "tools": [] })))?;
                } else {
                    let response = match load_snapshot() {
                        Ok(snapshot) => success(id, tools_list_result(profile, &snapshot)),
                        Err(message) => error(id, -32000, &message),
                    };
                    write_response(&output, response)?;
                }
            }
            "tools/call" => {
                log_timing(
                    "tools/call received",
                    &id,
                    params.get("name").and_then(Value::as_str).unwrap_or(""),
                    None,
                );
                let profile = tool_profile_from_env();
                let response_output = Arc::clone(&output);
                let response_state = Arc::clone(&state);
                let response_limiter = Arc::clone(&heavy_limiter);
                let response_id = id.clone();
                let submit = executor.submit(move || {
                    let response = execute_tool_call(
                        id,
                        profile,
                        params,
                        &response_state,
                        &response_limiter,
                    );
                    if let Err(message) = write_response(&response_output, response) {
                        tracing::error!(reason = %message, "lumina MCP failed to write tool response");
                    }
                });
                if let Err(message) = submit {
                    write_response(&output, error(response_id, -32000, &message))?;
                }
                continue;
            }
            "ping" => write_response(&output, success(id, json!({})))?,
            _ if id.is_null() => continue,
            other => write_response(
                &output,
                error(id, -32601, &format!("Method not found: {other}")),
            )?,
        };
    }
    Ok(())
}

fn initialize_result(params: Value) -> Value {
    let _ = params;
    initialize_result_for_profile(tool_profile_from_env())
}

fn initialize_result_for_profile(profile: McpToolProfile) -> Value {
    let mut result = json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "lumina", "version": env!("CARGO_PKG_VERSION") }
    });
    if profile != McpToolProfile::NoTools {
        result["instructions"] = json!(MCP_SERVER_INSTRUCTIONS);
    }
    result
}

fn tools_list_result(profile: McpToolProfile, snapshot: &LuminaMcpSnapshot) -> Value {
    let policy = ToolPolicy::new(profile);
    let tools: Vec<Value> = policy
        .tools(snapshot)
        .iter()
        .filter_map(|name| tool_json(name))
        .collect();
    json!({ "tools": tools })
}

/// Tool JSON schemas keyed by the canonical directory in `lumina-core`.
/// Returns `None` for unknown names (safe direction: omit, never invent).
/// Schemas are byte-identical to the pre-contract payloads; only the keying
/// moved from string literals to the shared contract. `annotations` are
/// attached afterwards by [`tool_read_only_hint`].
///
/// Concurrency signal for Codex App Server (v0.134+ partitions by
/// `readOnlyHint` when the server flag is absent): the nine read tools may
/// dispatch concurrently, the subtitle writer stays serial. `capture_frames`
/// only leaves temp files discarded on return; `propose_video_annotation`
/// never writes the notes store (the user confirms in UI first); both count
/// as read-only here.
fn tool_read_only_hint(name: &str) -> bool {
    use lumina_core::tool_contract as contract;
    matches!(
        name,
        contract::TOOL_PLAYBACK_CONTEXT
            | contract::TOOL_LIBRARY_CONTEXT
            | contract::TOOL_EPISODE_INDEX
            | contract::TOOL_TRANSCRIPT_WINDOW
            | contract::TOOL_EPISODE_TRANSCRIPT
            | contract::TOOL_AUDIO_MARKS
            | contract::TOOL_SUBTITLE_CUES
            | contract::TOOL_CAPTURE_FRAMES
            | contract::TOOL_PROPOSE_ANNOTATION
    )
}
fn tool_json(name: &str) -> Option<Value> {
    use lumina_core::tool_contract as contract;
    let mut tool = if name == contract::TOOL_PLAYBACK_CONTEXT {
        json!({
            "name": contract::TOOL_PLAYBACK_CONTEXT,
            "description": "Return frozen playback anchor and currentEpisode from the snapshot file.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_LIBRARY_CONTEXT {
        json!({
            "name": contract::TOOL_LIBRARY_CONTEXT,
            "description": "Return series metadata plus the current episode overview/plot. Loads from warm cache when present, otherwise reads local metadata on demand.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_EPISODE_INDEX {
        json!({
            "name": contract::TOOL_EPISODE_INDEX,
            "description": "List episode titles and overviews for every episode in the current series group.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_TRANSCRIPT_WINDOW {
        json!({
            "name": contract::TOOL_TRANSCRIPT_WINDOW,
            "description": "Return subtitle lines around a time point on the current media file. Defaults to the frozen prompt anchor; optional centerMs or atSec overrides the center. Window defaults to 60 seconds before and after (each side up to 300 seconds).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "centerMs": { "type": "integer", "minimum": 0 },
                    "atSec": { "type": "integer", "minimum": 0 },
                    "beforeSec": { "type": "integer", "minimum": 0, "maximum": 300 },
                    "afterSec": { "type": "integer", "minimum": 0, "maximum": 300 },
                    "radiusSec": { "type": "integer", "minimum": 0, "maximum": 300 }
                },
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_EPISODE_TRANSCRIPT {
        json!({
            "name": contract::TOOL_EPISODE_TRANSCRIPT,
            "description": "Return subtitle lines for another episode in the same series group. Requires season and episode. Files named EPxx without a season resolve as season 1, so S01E01 matches EP01. Defaults to the frozen prompt anchor when season/episode match the anchor file, otherwise episode start (0). Optional centerMs/atSec overrides. Uses the anchor subtitle track unless subtitleChoiceId is provided. Window defaults to 60 seconds before and after (each side up to 300 seconds).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "season": { "type": "integer", "minimum": 1 },
                    "episode": { "type": "integer", "minimum": 1 },
                    "centerMs": { "type": "integer", "minimum": 0 },
                    "atSec": { "type": "integer", "minimum": 0 },
                    "subtitleChoiceId": { "type": "string" },
                    "beforeSec": { "type": "integer", "minimum": 0, "maximum": 300 },
                    "afterSec": { "type": "integer", "minimum": 0, "maximum": 300 },
                    "radiusSec": { "type": "integer", "minimum": 0, "maximum": 300 }
                },
                "required": ["season", "episode"],
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_AUDIO_MARKS {
        json!({
            "name": contract::TOOL_AUDIO_MARKS,
            "description": "Return non-semantic audio signals around a time point on the anchor media file: silence intervals and loudness-spike candidates with millisecond timestamps. These are NOT laughter/applause/music labels. Defaults to the frozen prompt anchor; optional centerMs or atSec overrides the center. Window defaults to 60 seconds before and after (each side up to 300 seconds).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "centerMs": { "type": "integer", "minimum": 0 },
                    "atSec": { "type": "integer", "minimum": 0 },
                    "beforeSec": { "type": "integer", "minimum": 0, "maximum": 300 },
                    "afterSec": { "type": "integer", "minimum": 0, "maximum": 300 },
                    "radiusSec": { "type": "integer", "minimum": 0, "maximum": 300 }
                },
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_SUBTITLE_CUES {
        json!({
            "name": contract::TOOL_SUBTITLE_CUES,
            "description": "Return a page of full subtitle cues for the anchor media (for translation/workshop). Defaults to the frozen subtitleChoiceId. Use offset/limit (default 80, max 200) to batch line-by-line work.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "subtitleChoiceId": { "type": "string" },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_WRITE_SUBTITLE_TRACK {
        json!({
            "name": contract::TOOL_WRITE_SUBTITLE_TRACK,
            "description": "Write a new sidecar subtitle track beside the anchor media as {stem}.{lang}.srt. Provide lang token (e.g. en/zh) and cues with startMs/endMs/text (timings usually copied from source). Use after translating one batch or the full page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "lang": { "type": "string", "minLength": 1, "maxLength": 24 },
                    "cues": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "index": { "type": "integer", "minimum": 0 },
                                "startMs": { "type": "integer", "minimum": 0 },
                                "endMs": { "type": "integer", "minimum": 0 },
                                "text": { "type": "string" }
                            },
                            "required": ["startMs", "endMs", "text"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["lang", "cues"],
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_CAPTURE_FRAMES {
        json!({
            "name": contract::TOOL_CAPTURE_FRAMES,
            "description": "Capture temporary JPEG frames (~1 per second) around a time point on the anchor media file. Defaults to the frozen prompt anchor; optional centerMs or atSec overrides the center. Default is a single frame at center. Use radiusSec or beforeSec/afterSec (each capped at 7s, max 15 frames). Files are discarded after the tool returns.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "centerMs": { "type": "integer", "minimum": 0 },
                    "atSec": { "type": "integer", "minimum": 0 },
                    "beforeSec": { "type": "integer", "minimum": 0, "maximum": 7 },
                    "afterSec": { "type": "integer", "minimum": 0, "maximum": 7 },
                    "radiusSec": { "type": "integer", "minimum": 0, "maximum": 7 }
                },
                "additionalProperties": false
            }
        })
    } else if name == contract::TOOL_PROPOSE_ANNOTATION {
        json!({
            "name": contract::TOOL_PROPOSE_ANNOTATION,
            "description": "Propose a timestamped video annotation with optional quoted subtitle lines. Does NOT write to the notes store — the user must confirm in Lumina UI before it is saved. Call after you have enough context (transcript window, playback anchor). Provide body (required); optional positionMs, anchorCueIndex, quoteCueIndices, quoteHint, includeQuotes, subtitleChoiceId.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "body": { "type": "string", "minLength": 1 },
                    "positionMs": { "type": "integer", "minimum": 0 },
                    "subtitleChoiceId": { "type": "string" },
                    "anchorCueIndex": { "type": "integer", "minimum": 0 },
                    "quoteCueIndices": {
                        "type": "array",
                        "items": { "type": "integer", "minimum": 0 }
                    },
                    "quoteHint": { "type": "string" },
                    "includeQuotes": { "type": "boolean" }
                },
                "required": ["body"],
                "additionalProperties": false
            }
        })
    } else {
        return None;
    };
    tool["annotations"] = json!({ "readOnlyHint": tool_read_only_hint(name) });
    Some(tool)
}

fn handle_tool_call_request(
    profile: McpToolProfile,
    snapshot: &LuminaMcpSnapshot,
    params: &Value,
) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call missing name".to_string())?;
    // Policy runs before dispatch: a hand-written name for an unauthorized
    // tool is rejected instead of executed.
    ToolPolicy::new(profile).check(snapshot, name)?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    handle_tool_call(snapshot, name, &args)
}

fn execute_tool_call(
    id: Value,
    profile: McpToolProfile,
    params: Value,
    state: &ToolState,
    heavy_limiter: &Arc<HeavyToolLimiter>,
) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let started = std::time::Instant::now();
    log_timing("tool started", &id, name, None);
    let result = if is_read_only_tool(name) {
        let _state_guard = match state.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if is_heavy_read_only_tool(name) {
            let _heavy_permit = heavy_limiter.acquire();
            dispatch_tool_call(profile, &params)
        } else {
            dispatch_tool_call(profile, &params)
        }
    } else {
        // Writes and unknown tools take the exclusive path. Treating unknown
        // names conservatively prevents a future side-effecting tool from
        // accidentally bypassing the serialization boundary.
        let _state_guard = match state.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        dispatch_tool_call(profile, &params)
    };

    log_timing(
        "tool finished",
        &id,
        name,
        Some(started.elapsed().as_millis()),
    );
    match result {
        Ok(result) => success(id, result),
        Err(message) => error(id, -32000, &message),
    }
}

fn dispatch_tool_call(profile: McpToolProfile, params: &Value) -> Result<Value, String> {
    if profile == McpToolProfile::NoTools {
        let denied = match params.get("name").and_then(Value::as_str) {
            Some(name) => match ToolPolicy::new(profile).check(&LuminaMcpSnapshot::empty(), name) {
                Ok(()) => "该工具未对当前任务开放".to_string(),
                Err(message) => message,
            },
            None => "tools/call missing name".to_string(),
        };
        return Ok(tool_error_result(&denied));
    }

    let snapshot = load_snapshot().map_err(|message| {
        tracing::warn!(reason = %message, "lumina MCP snapshot unavailable");
        message
    })?;
    match handle_tool_call_request(profile, &snapshot, params) {
        Ok(result) => Ok(result),
        Err(message) => {
            tracing::warn!(
                tool = params
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
                reason = %message,
                "lumina MCP tools/call returned business error"
            );
            Ok(tool_error_result(&message))
        }
    }
}

fn is_read_only_tool(name: &str) -> bool {
    use lumina_core::tool_contract as contract;
    [
        contract::TOOL_PLAYBACK_CONTEXT,
        contract::TOOL_LIBRARY_CONTEXT,
        contract::TOOL_EPISODE_INDEX,
        contract::TOOL_TRANSCRIPT_WINDOW,
        contract::TOOL_EPISODE_TRANSCRIPT,
        contract::TOOL_AUDIO_MARKS,
        contract::TOOL_SUBTITLE_CUES,
        contract::TOOL_CAPTURE_FRAMES,
    ]
    .contains(&name)
}

fn is_heavy_read_only_tool(name: &str) -> bool {
    use lumina_core::tool_contract as contract;
    [contract::TOOL_AUDIO_MARKS, contract::TOOL_CAPTURE_FRAMES].contains(&name)
}

fn write_response(output: &Output, response: Value) -> Result<(), String> {
    let mut stdout = match output.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let serialized = serde_json::to_string(&response)
        .map_err(|error| format!("MCP response serialization failed: {error}"))?;
    writeln!(stdout, "{serialized}").map_err(|error| format!("stdout write: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("stdout flush: {error}"))
}

struct HeavyToolLimiter {
    available: Mutex<usize>,
    wake: Condvar,
}

impl HeavyToolLimiter {
    fn new(limit: usize) -> Self {
        Self {
            available: Mutex::new(limit.max(1)),
            wake: Condvar::new(),
        }
    }

    fn acquire(self: &Arc<Self>) -> HeavyToolPermit {
        let mut available = match self.available.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        while *available == 0 {
            available = match self.wake.wait(available) {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
        *available -= 1;
        HeavyToolPermit {
            limiter: Arc::clone(self),
        }
    }
}

struct HeavyToolPermit {
    limiter: Arc<HeavyToolLimiter>,
}

impl Drop for HeavyToolPermit {
    fn drop(&mut self) {
        let mut available = match self.limiter.available.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *available += 1;
        self.limiter.wake.notify_one();
    }
}

fn tool_error_result(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
}

fn load_snapshot() -> Result<LuminaMcpSnapshot, String> {
    let path = resolve_snapshot_path()
        .ok_or_else(|| "LUMINA_MCP_CONTEXT_FILE or cwd unavailable".to_string())?;
    read_snapshot(&path)
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{AgentCapabilities, LuminaMcpSnapshot, SNAPSHOT_SCHEMA_VERSION};

    #[test]
    fn tool_list_matches_canonical_contract() {
        use lumina_core::tool_contract as contract;
        // Every canonical name resolves to a schema whose `name` echoes it;
        // unknown names still resolve to `None` (omit, never invent).
        assert_eq!(contract::ALL_TOOLS.len(), contract::TOOL_COUNT);
        for name in contract::ALL_TOOLS {
            let tool = tool_json(name).expect("canonical tool must have a schema");
            assert_eq!(tool.get("name").and_then(Value::as_str), Some(*name));
            assert!(tool.get("description").and_then(Value::as_str).is_some());
            assert!(tool.get("inputSchema").is_some());
            // Concurrency signal: only the subtitle writer mutates disk.
            let read_only = tool
                .pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool)
                .expect("annotations.readOnlyHint must be present");
            assert_eq!(
                read_only,
                *name != contract::TOOL_WRITE_SUBTITLE_TRACK,
                "readOnlyHint wrong for {name}"
            );
        }
        assert!(tool_json("lumina_do_anything").is_none());
        // Schema bounds mirror the canonical numeric contract.
        let window = tool_json(contract::TOOL_TRANSCRIPT_WINDOW).expect("window schema");
        let props = window
            .pointer("/inputSchema/properties/beforeSec")
            .expect("beforeSec schema");
        assert_eq!(
            props.get("maximum").and_then(Value::as_u64),
            Some(u64::from(contract::WINDOW_MAX_SEC))
        );
        let capture = tool_json(contract::TOOL_CAPTURE_FRAMES).expect("capture schema");
        let before = capture
            .pointer("/inputSchema/properties/beforeSec")
            .expect("capture beforeSec");
        assert_eq!(
            before.get("maximum").and_then(Value::as_u64),
            Some(u64::from(contract::CAPTURE_MAX_SEC))
        );
        let cues = tool_json(contract::TOOL_SUBTITLE_CUES).expect("cues schema");
        let limit = cues
            .pointer("/inputSchema/properties/limit")
            .expect("limit schema");
        assert_eq!(
            limit.get("maximum").and_then(Value::as_u64),
            Some(contract::SUBTITLE_CUES_MAX_LIMIT as u64)
        );
        let write = tool_json(contract::TOOL_WRITE_SUBTITLE_TRACK).expect("write schema");
        let lang = write
            .pointer("/inputSchema/properties/lang")
            .expect("lang schema");
        assert_eq!(
            lang.get("maxLength").and_then(Value::as_u64),
            Some(contract::WRITE_LANG_MAX_LEN as u64)
        );
        let episode = tool_json(contract::TOOL_EPISODE_TRANSCRIPT).expect("episode schema");
        let season = episode
            .pointer("/inputSchema/properties/season")
            .expect("season schema");
        assert_eq!(
            season.get("minimum").and_then(Value::as_u64),
            Some(u64::from(contract::SEASON_EPISODE_MIN))
        );
        // Policy directory and dispatch agree with the contract on identity.
        for name in contract::ALL_TOOLS {
            assert!(
                super::super::policy::allowed_tool_names(
                    McpToolProfile::SubtitleWorkshop,
                    &LuminaMcpSnapshot::empty()
                )
                .contains(name)
                    || {
                        let gated = LuminaMcpSnapshot {
                            capabilities: Some(AgentCapabilities {
                                vision_capable: true,
                                subtitle_workshop_enabled: true,
                                video_annotations_enabled: true,
                            }),
                            ..LuminaMcpSnapshot::empty()
                        };
                        super::super::policy::allowed_tool_names(McpToolProfile::Chat, &gated)
                            .contains(name)
                    },
                "contract tool must be reachable via policy: {name}"
            );
        }
    }

    #[test]
    fn initialize_exposes_stable_instructions_only_for_tool_profiles() {
        let result = initialize_result_for_profile(McpToolProfile::Chat);
        let instructions = result
            .get("instructions")
            .and_then(Value::as_str)
            .expect("tool profile should expose MCP instructions");
        assert!(instructions.contains("tools/list"));
        // Eager catalog: all identities known before tools/list.
        for name in lumina_core::tool_contract::ALL_TOOLS {
            assert!(
                instructions.contains(name),
                "instructions must pre-feed tool: {name}"
            );
        }
        assert!(instructions.contains("lumina_get_transcript_window"));
        assert!(instructions.contains("lumina_propose_video_annotation"));
        assert!(instructions.contains("lumina_capture_frames"));
        // Mandatory trigger rules: plot must hit tools first, no web-first.
        assert!(instructions.contains("必须先调"));
        assert!(instructions.contains("禁止先走网络搜索"));
        // Gated-tool guidance: missing from list means unavailable, never guess.
        assert!(instructions.contains("不在 tools/list 中"));
        assert!(!instructions.contains("mediaPath"));
        assert!(!instructions.contains("turn 数"));

        let restricted = initialize_result_for_profile(McpToolProfile::NoTools);
        assert!(restricted.get("instructions").is_none());
    }

    #[test]
    fn tool_concurrency_classification_keeps_mutations_exclusive() {
        use lumina_core::tool_contract as contract;

        assert!(is_read_only_tool(contract::TOOL_PLAYBACK_CONTEXT));
        assert!(is_read_only_tool(contract::TOOL_SUBTITLE_CUES));
        assert!(is_read_only_tool(contract::TOOL_CAPTURE_FRAMES));
        assert!(is_heavy_read_only_tool(contract::TOOL_CAPTURE_FRAMES));
        assert!(is_heavy_read_only_tool(contract::TOOL_AUDIO_MARKS));

        assert!(!is_read_only_tool(contract::TOOL_WRITE_SUBTITLE_TRACK));
        assert!(!is_read_only_tool(contract::TOOL_PROPOSE_ANNOTATION));
        assert!(!is_read_only_tool("lumina_unknown_tool"));
    }

    #[test]
    fn tools_list_contains_core_tools() {
        let snapshot = LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: None,
            current_episode: None,
            library: None,
            session: None,
            capabilities: Some(AgentCapabilities {
                vision_capable: false,
                subtitle_workshop_enabled: false,
                video_annotations_enabled: true,
            }),
            online: None,
            updated_at_ms: 0,
        };
        let tools = tools_list_result(McpToolProfile::Chat, &snapshot)
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"lumina_get_playback_context"));
        assert!(names.contains(&"lumina_get_transcript_window"));
        assert!(names.contains(&"lumina_get_episode_transcript"));
        assert!(names.contains(&"lumina_get_audio_marks"));
        assert!(names.contains(&"lumina_propose_video_annotation"));
        assert!(!names.contains(&"lumina_get_subtitle_cues"));
        assert!(!names.contains(&"lumina_write_subtitle_track"));
        assert!(!names.contains(&"lumina_capture_frames"));
    }

    #[test]
    fn tools_list_includes_workshop_tools_when_enabled() {
        let snapshot = LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: None,
            current_episode: None,
            library: None,
            session: None,
            capabilities: Some(AgentCapabilities {
                vision_capable: false,
                subtitle_workshop_enabled: true,
                video_annotations_enabled: false,
            }),
            online: None,
            updated_at_ms: 0,
        };
        let tools = tools_list_result(McpToolProfile::Chat, &snapshot)
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"lumina_get_subtitle_cues"));
        assert!(names.contains(&"lumina_write_subtitle_track"));
    }

    #[test]
    fn tools_list_includes_capture_when_vision_enabled() {
        let snapshot = LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: None,
            current_episode: None,
            library: None,
            session: None,
            capabilities: Some(AgentCapabilities {
                vision_capable: true,
                subtitle_workshop_enabled: false,
                video_annotations_enabled: true,
            }),
            online: None,
            updated_at_ms: 0,
        };
        let tools = tools_list_result(McpToolProfile::Chat, &snapshot)
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"lumina_capture_frames"));
    }

    fn full_snapshot() -> LuminaMcpSnapshot {
        LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: None,
            current_episode: None,
            library: None,
            session: None,
            capabilities: Some(AgentCapabilities {
                vision_capable: true,
                subtitle_workshop_enabled: true,
                video_annotations_enabled: true,
            }),
            online: None,
            updated_at_ms: 0,
        }
    }

    fn list_names(profile: McpToolProfile, snapshot: &LuminaMcpSnapshot) -> Vec<String> {
        tools_list_result(profile, snapshot)
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect()
    }

    #[test]
    fn workshop_profile_lists_only_subtitle_tools() {
        let names = list_names(McpToolProfile::SubtitleWorkshop, &full_snapshot());
        assert_eq!(
            names,
            vec![
                "lumina_get_subtitle_cues".to_string(),
                "lumina_write_subtitle_track".to_string(),
            ]
        );
    }

    #[test]
    fn restricted_profiles_list_nothing() {
        let snapshot = full_snapshot();
        assert!(list_names(McpToolProfile::MetadataResolver, &snapshot).is_empty());
        assert!(list_names(McpToolProfile::NoTools, &snapshot).is_empty());
    }

    #[test]
    fn unauthorized_tool_call_is_rejected() {
        let snapshot = full_snapshot();
        // NoTools serves nothing, not even core tools.
        let denied = handle_tool_call_request(
            McpToolProfile::NoTools,
            &snapshot,
            &json!({"name": "lumina_get_transcript_window"}),
        )
        .expect_err("no-tools denies core tool");
        assert!(denied.contains("未对当前任务开放"));
        // Unknown names keep the historical error instead of executing.
        let unknown = handle_tool_call_request(
            McpToolProfile::Chat,
            &snapshot,
            &json!({"name": "lumina_do_anything"}),
        )
        .expect_err("unknown tool");
        assert!(unknown.contains("Unknown tool"));
    }

    #[test]
    fn chat_respects_snapshot_gates_on_call() {
        let mut snapshot = full_snapshot();
        snapshot.capabilities = Some(AgentCapabilities {
            vision_capable: false,
            subtitle_workshop_enabled: false,
            video_annotations_enabled: true,
        });
        // Hand-written vision/workshop names are rejected when the snapshot
        // does not enable them, even though the names exist.
        let denied = handle_tool_call_request(
            McpToolProfile::Chat,
            &snapshot,
            &json!({"name": "lumina_capture_frames"}),
        )
        .expect_err("vision-gated call denied");
        assert!(denied.contains("未对当前任务开放"));
        let denied = handle_tool_call_request(
            McpToolProfile::Chat,
            &snapshot,
            &json!({"name": "lumina_write_subtitle_track"}),
        )
        .expect_err("workshop-gated call denied");
        assert!(denied.contains("未对当前任务开放"));
    }
}
