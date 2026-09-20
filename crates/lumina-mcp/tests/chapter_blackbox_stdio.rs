use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumina_library::{Database, NewAgentAttempt, NewAgentTask, NewChapter, NewEpisode, NewSeries};
use lumina_mcp::{
    write_chapter_task_snapshot, AgentCapabilities, ChapterTaskContext, LuminaMcpSnapshot,
    OnlineMediaSnapshot, PromptAnchor,
};
use lumina_subtitle::model::{Cue, SubtitleChoice, SubtitleSource, Transcript};
use serde::Serialize;
use serde_json::{json, Value};

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);
const DELAY_ENV: &str = "LUMINA_CHAPTER_BLACKBOX_DELAY_MS";
const ARTIFACT_DIR_ENV: &str = "LUMINA_CHAPTER_BLACKBOX_ARTIFACT_DIR";
const CAPTURE_FIXTURE_ENV: &str = "LUMINA_MCP_TEST_CAPTURE_FIXTURE_DIR";
const SERVER_DIAGNOSTIC_ENV: &str = "LUMINA_MCP_DIAGNOSTIC_LOG";
const FRAME_JPEG: &[u8] = b"\xff\xd8\xff\xe0\0\x10JFIF\0\x01\x01\0\0\x01\0\x01\0\0\xff\xdb\0\x43\0\x08\x06\x06\x07\x06\x05\x08\x07\x07\x07\x09\x09\x08\x0a\x0c\x14\x0d\x0c\x0b\x0b\x0c\x19\x12\x13\x0f\x14\x1d\x1a\x1f\x1e\x1d\x1a\x1c\x1c\x20\x24\x2e\x27\x20\x22\x2c\x23\x1c\x1c\x28\x37\x29\x2c\x30\x31\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\x34\xff\xc0\0\x11\x08\0\x01\0\x01\x01\x01\x11\0\0\x01\x11\0\0\xff\xc4\0\x14\0\x01\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\xff\xc4\0\x14\x10\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\xff\xda\0\x08\x01\x01\0\0\x3f\0\xd2\xcf\x20\xff\xd9";

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct LogEvent {
    scenario: String,
    task_id: i64,
    attempt_id: i64,
    turn: u32,
    phase: String,
    tool: Option<String>,
    elapsed_ms: u128,
    delay_ms: u64,
    final_status: Option<String>,
    failure_reason: Option<String>,
}

struct JsonlLog {
    path: PathBuf,
    file: File,
    scenario_started: Instant,
    scenario: String,
    task_id: i64,
    attempt_id: i64,
}

struct LogDetails<'a> {
    turn: u32,
    phase: &'a str,
    tool: Option<&'a str>,
    elapsed_ms: u128,
    delay_ms: u64,
    final_status: Option<&'a str>,
    failure_reason: Option<&'a str>,
}

impl JsonlLog {
    fn new(root: &Path, scenario: &str, task_id: i64, attempt_id: i64) -> Self {
        let path = root.join(format!("{scenario}.jsonl"));
        let file = File::create(&path).unwrap_or_else(|error| panic!("create JSONL log: {error}"));
        Self {
            path,
            file,
            scenario_started: Instant::now(),
            scenario: scenario.to_owned(),
            task_id,
            attempt_id,
        }
    }

    fn event(&mut self, details: LogDetails<'_>) {
        let event = LogEvent {
            scenario: self.scenario.clone(),
            task_id: self.task_id,
            attempt_id: self.attempt_id,
            turn: details.turn,
            phase: details.phase.to_owned(),
            tool: details.tool.map(str::to_owned),
            elapsed_ms: details.elapsed_ms,
            delay_ms: details.delay_ms,
            final_status: details.final_status.map(str::to_owned),
            failure_reason: details.failure_reason.map(str::to_owned),
        };
        let line =
            serde_json::to_string(&event).unwrap_or_else(|error| panic!("encode JSONL: {error}"));
        writeln!(self.file, "{line}").unwrap_or_else(|error| panic!("write JSONL: {error}"));
        self.file
            .flush()
            .unwrap_or_else(|error| panic!("flush JSONL: {error}"));
    }

    fn final_status(&mut self, turn: u32, status: &str, reason: Option<&str>) {
        self.event(LogDetails {
            turn,
            phase: "scenario",
            tool: None,
            elapsed_ms: self.scenario_started.elapsed().as_millis(),
            delay_ms: 0,
            final_status: Some(status),
            failure_reason: reason,
        });
        if let Some(artifact_dir) = std::env::var_os(ARTIFACT_DIR_ENV)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(&artifact_dir)
                .unwrap_or_else(|error| panic!("create artifact directory: {error}"));
            let destination = artifact_dir.join(format!("{}.jsonl", self.scenario));
            fs::copy(&self.path, &destination)
                .unwrap_or_else(|error| panic!("copy JSONL artifact: {error}"));
        }
    }
}

struct Fixture {
    root: PathBuf,
    snapshot_path: PathBuf,
    database_path: PathBuf,
    capture_dir: PathBuf,
    context: ChapterTaskContext,
    foreign_chapter_id: i64,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fixture(scenario: &str) -> Fixture {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "lumina-chapter-blackbox-{scenario}-{}-{sequence}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap_or_else(|error| panic!("fixture root: {error}"));
    let database_path = root.join("chapters.sqlite3");
    let media_path = root.join("deterministic-media.input");
    let snapshot_path = root.join("chapter-agent-context.json");
    let capture_dir = root.join("fixed-frames");
    fs::write(&media_path, b"deterministic test media input")
        .unwrap_or_else(|error| panic!("media fixture: {error}"));
    fs::create_dir_all(&capture_dir).unwrap_or_else(|error| panic!("capture fixture: {error}"));
    fs::write(capture_dir.join("frame.jpg"), FRAME_JPEG)
        .unwrap_or_else(|error| panic!("JPEG fixture: {error}"));

    let (task_id, attempt_id, episode_id, foreign_chapter_id) = {
        let database = Database::open(&database_path)
            .unwrap_or_else(|error| panic!("database: {}", error.message));
        let repository = database.repository();
        let series_id = repository
            .insert_series(&NewSeries::new(
                "fixture-series",
                "Fixture series",
                "blackbox",
            ))
            .unwrap_or_else(|error| panic!("series: {}", error.message));
        let mut episode = NewEpisode::new(series_id, "s01e01", "blackbox");
        episode.duration_ms = Some(12_000);
        let episode_id = repository
            .insert_episode(&episode)
            .unwrap_or_else(|error| panic!("episode: {}", error.message));
        let mut task = NewAgentTask::new(
            "blackbox-chapter-task",
            "chapter_generation",
            "chapter-agent.v1",
        );
        task.episode_id = Some(episode_id);
        task.status = "running".to_owned();
        let task_id = repository
            .insert_agent_task(&task)
            .unwrap_or_else(|error| panic!("agent task: {}", error.message));
        let attempt_id = repository
            .insert_agent_attempt(&NewAgentAttempt::new(
                task_id,
                1,
                "initial",
                "running",
                "chapter-agent.v1",
                1,
            ))
            .unwrap_or_else(|error| panic!("agent attempt: {}", error.message));

        let mut foreign_episode = NewEpisode::new(series_id, "s01e99", "blackbox");
        foreign_episode.duration_ms = Some(12_000);
        let foreign_episode_id = repository
            .insert_episode(&foreign_episode)
            .unwrap_or_else(|error| panic!("foreign episode: {}", error.message));
        let foreign_chapter_id = repository
            .insert_chapter(&NewChapter::new(
                foreign_episode_id,
                "foreign-chapter",
                0,
                4_000,
                "fixture",
            ))
            .unwrap_or_else(|error| panic!("foreign chapter: {}", error.message));
        (task_id, attempt_id, episode_id, foreign_chapter_id)
    };

    let choice_id = "fixture:zh".to_owned();
    let context = ChapterTaskContext {
        task_id,
        attempt_id,
        episode_id,
        database_path: database_path.to_string_lossy().into_owned(),
        media_path: media_path.to_string_lossy().into_owned(),
        duration_ms: 12_000,
        spoiler_boundary: "episode".to_owned(),
        prompt_version: "chapter-agent.v1".to_owned(),
    };
    let snapshot = LuminaMcpSnapshot {
        anchor: Some(PromptAnchor {
            media_path: media_path.to_string_lossy().into_owned(),
            media_title: Some("Deterministic fixture".to_owned()),
            library_root: None,
            group_key: None,
            season: Some(1),
            episode: Some(1),
            position_ms: 2_000,
            duration_ms: Some(12_000),
            sent_at_ms: 1,
            subtitle_choice_id: Some(choice_id.clone()),
            transcript_window_radius_sec: Some(3),
        }),
        capabilities: Some(AgentCapabilities {
            vision_capable: true,
            subtitle_workshop_enabled: false,
            video_annotations_enabled: false,
        }),
        online: Some(OnlineMediaSnapshot {
            media_id: "fixture-media".to_owned(),
            title: Some("Deterministic fixture".to_owned()),
            duration_ms: Some(12_000),
            webpage_url: None,
            extractor: None,
            chapters: Vec::new(),
            subtitles: vec![SubtitleChoice {
                id: choice_id.clone(),
                source: SubtitleSource::Sidecar,
                label: "固定中文".to_owned(),
                supported: true,
                stream_index: None,
                external_path: None,
                codec_name: Some("srt".to_owned()),
                language: Some("zh-CN".to_owned()),
            }],
            transcript: Some(Transcript {
                source_path: "fixture.srt".to_owned(),
                choice_id,
                stream_index: None,
                language: Some("zh-CN".to_owned()),
                codec_name: Some("srt".to_owned()),
                cues: vec![
                    Cue {
                        index: 0,
                        start_ms: 500,
                        end_ms: 1_500,
                        text: "固定字幕：门在雨夜打开。".to_owned(),
                    },
                    Cue {
                        index: 1,
                        start_ms: 2_000,
                        end_ms: 3_000,
                        text: "固定字幕：两人决定继续调查。".to_owned(),
                    },
                ],
            }),
        }),
        ..LuminaMcpSnapshot::empty()
    };
    write_chapter_task_snapshot(&snapshot_path, &snapshot, &context)
        .unwrap_or_else(|error| panic!("chapter snapshot: {error}"));
    Fixture {
        root,
        snapshot_path,
        database_path,
        capture_dir,
        context,
        foreign_chapter_id,
    }
}

struct McpDriver {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    turn: u32,
    delay_ms: u64,
}

impl McpDriver {
    fn spawn(fixture: &Fixture, log: &mut JsonlLog) -> Self {
        let diagnostic_path = fixture.root.join("lumina-mcp-diagnostic.log");
        let mut command = Command::new(env!("CARGO_BIN_EXE_lumina-mcp-stdio-test-server"));
        command
            .arg("--lumina-mcp")
            .env("LUMINA_MCP_CONTEXT_FILE", &fixture.snapshot_path)
            .env("LUMINA_MCP_TOOL_PROFILE", "chat")
            .env(CAPTURE_FIXTURE_ENV, &fixture.capture_dir)
            .env(SERVER_DIAGNOSTIC_ENV, &diagnostic_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .unwrap_or_else(|error| panic!("spawn MCP server: {error}"));
        let stdin = child.stdin.take().unwrap_or_else(|| panic!("MCP stdin"));
        let stdout = child.stdout.take().unwrap_or_else(|| panic!("MCP stdout"));
        let delay_ms = std::env::var(DELAY_ENV)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        log.event(LogDetails {
            turn: 0,
            phase: "server_spawn",
            tool: None,
            elapsed_ms: 0,
            delay_ms: 0,
            final_status: None,
            failure_reason: None,
        });
        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            turn: 0,
            delay_ms,
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        phase: &str,
        tool: Option<&str>,
        log: &mut JsonlLog,
    ) -> Value {
        self.turn += 1;
        let id = self.turn;
        if self.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }
        let started = Instant::now();
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let line = serde_json::to_string(&request)
            .unwrap_or_else(|error| panic!("encode request: {error}"));
        writeln!(self.stdin, "{line}").unwrap_or_else(|error| panic!("write MCP request: {error}"));
        self.stdin
            .flush()
            .unwrap_or_else(|error| panic!("flush MCP request: {error}"));
        let mut response_line = String::new();
        self.stdout
            .read_line(&mut response_line)
            .unwrap_or_else(|error| panic!("read MCP response: {error}"));
        let response: Value = serde_json::from_str(&response_line)
            .unwrap_or_else(|error| panic!("decode MCP response: {error}: {response_line}"));
        assert_eq!(
            response.get("id").and_then(Value::as_i64),
            Some(i64::from(id))
        );
        let elapsed_ms = started.elapsed().as_millis();
        log.event(LogDetails {
            turn: self.turn,
            phase,
            tool,
            elapsed_ms,
            delay_ms: self.delay_ms,
            final_status: None,
            failure_reason: None,
        });
        response
    }

    fn initialize_and_list(&mut self, log: &mut JsonlLog) -> Value {
        let initialize = self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "chapter-blackbox-mock-agent", "version": "1" }
            }),
            "initialize",
            None,
            log,
        );
        assert!(
            initialize.get("result").is_some(),
            "initialize failed: {initialize}"
        );
        self.notify("notifications/initialized", json!({}), "initialized", log);
        let listed = self.request("tools/list", json!({}), "tools_list", None, log);
        let tools = listed["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("tools/list failed: {listed}"));
        for name in [
            "lumina_create_chapter_outline",
            "lumina_capture_chapter_evidence",
            "lumina_update_chapter_draft",
            "lumina_finalize_chapter_task",
        ] {
            assert!(
                tools.iter().any(|tool| tool["name"] == name),
                "missing {name} in tools/list"
            );
        }
        listed
    }

    fn notify(&mut self, method: &str, params: Value, phase: &str, log: &mut JsonlLog) {
        if self.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }
        let started = Instant::now();
        let request = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let line = serde_json::to_string(&request)
            .unwrap_or_else(|error| panic!("encode notification: {error}"));
        writeln!(self.stdin, "{line}")
            .unwrap_or_else(|error| panic!("write notification: {error}"));
        self.stdin
            .flush()
            .unwrap_or_else(|error| panic!("flush notification: {error}"));
        log.event(LogDetails {
            turn: self.turn,
            phase,
            tool: None,
            elapsed_ms: started.elapsed().as_millis(),
            delay_ms: self.delay_ms,
            final_status: None,
            failure_reason: None,
        });
    }

    fn call(&mut self, tool: &str, arguments: Value, phase: &str, log: &mut JsonlLog) -> Value {
        let response = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
            phase,
            Some(tool),
            log,
        );
        assert!(
            response.get("result").is_some(),
            "tools/call transport error: {response}"
        );
        response["result"].clone()
    }
}

impl Drop for McpDriver {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text_payload(result: &Value) -> Value {
    let text = result["content"]
        .as_array()
        .and_then(|blocks| blocks.first())
        .and_then(|block| block["text"].as_str())
        .unwrap_or_else(|| panic!("missing MCP text payload: {result}"));
    serde_json::from_str(text).unwrap_or_else(|error| panic!("decode MCP text payload: {error}"))
}

fn is_tool_error(result: &Value) -> bool {
    result["isError"].as_bool() == Some(true)
}

fn assert_jsonl_contract(log: &JsonlLog, expected_status: &str) {
    let content =
        fs::read_to_string(&log.path).unwrap_or_else(|error| panic!("read JSONL: {error}"));
    let events = content
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("invalid JSONL: {error}"))
        })
        .collect::<Vec<_>>();
    assert!(!events.is_empty());
    assert!(events.iter().all(|event| {
        event.get("phase").is_some()
            && event.get("tool").is_some()
            && event.get("elapsed_ms").is_some()
            && event.get("delay_ms").is_some()
    }));
    let expected_delay = std::env::var(DELAY_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    assert!(events
        .iter()
        .filter(|event| event["phase"] != "server_spawn" && event["final_status"].is_null())
        .all(|event| event["delay_ms"] == expected_delay));
    assert_eq!(
        events
            .last()
            .and_then(|event| event["final_status"].as_str()),
        Some(expected_status)
    );
    let final_event = events
        .last()
        .unwrap_or_else(|| panic!("missing final JSONL event"));
    assert!(final_event["elapsed_ms"]
        .as_u64()
        .is_some_and(|elapsed_ms| elapsed_ms > 0));
}

fn outline(driver: &mut McpDriver, log: &mut JsonlLog) -> Vec<i64> {
    let result = driver.call(
        "lumina_create_chapter_outline",
        json!({
            "chapters": [
                { "stableId": "opening", "startMs": 0, "endMs": 4_000, "title": "雨夜开门" },
                { "stableId": "decision", "startMs": 4_000, "endMs": 8_000, "title": "决定调查" }
            ]
        }),
        "outline",
        log,
    );
    assert!(!is_tool_error(&result), "outline failed: {result}");
    text_payload(&result)["chapters"]
        .as_array()
        .unwrap_or_else(|| panic!("outline payload: {result}"))
        .iter()
        .map(|chapter| {
            chapter["chapterId"]
                .as_i64()
                .unwrap_or_else(|| panic!("chapter id: {chapter}"))
        })
        .collect()
}

#[test]
fn chapter_mcp_stdio_happy_path_projects_outline_evidence_draft_and_finalize() {
    let fixture = fixture("happy-path");
    let mut log = JsonlLog::new(
        &fixture.root,
        "happy_path",
        fixture.context.task_id,
        fixture.context.attempt_id,
    );
    let mut driver = McpDriver::spawn(&fixture, &mut log);
    driver.initialize_and_list(&mut log);

    let transcript = driver.call(
        "lumina_get_transcript_window",
        json!({ "beforeSec": 2, "afterSec": 2 }),
        "evidence",
        &mut log,
    );
    assert!(
        !is_tool_error(&transcript),
        "transcript failed: {transcript}"
    );
    assert!(serde_json::to_string(&text_payload(&transcript))
        .unwrap()
        .contains("固定字幕"));

    let chapters = outline(&mut driver, &mut log);
    let duplicate = driver.call(
        "lumina_create_chapter_outline",
        json!({
            "chapters": [
                { "stableId": "opening", "startMs": 0, "endMs": 4_000, "title": "雨夜开门" },
                { "stableId": "decision", "startMs": 4_000, "endMs": 8_000, "title": "决定调查" }
            ]
        }),
        "outline_retry",
        &mut log,
    );
    let duplicate_ids = text_payload(&duplicate)["chapters"]
        .as_array()
        .unwrap_or_else(|| panic!("duplicate outline: {duplicate}"))
        .iter()
        .map(|chapter| chapter["chapterId"].as_i64().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(duplicate_ids, chapters);

    let mut asset_ids = Vec::new();
    for (index, chapter_id) in chapters.iter().enumerate() {
        let timestamp_ms = if index == 0 { 2_000 } else { 6_000 };
        let evidence = driver.call(
            "lumina_capture_chapter_evidence",
            json!({ "chapterId": chapter_id, "timestampsMs": [timestamp_ms] }),
            "evidence",
            &mut log,
        );
        assert!(!is_tool_error(&evidence), "evidence failed: {evidence}");
        assert!(evidence["content"]
            .as_array()
            .is_some_and(|blocks| blocks.iter().any(|block| block["type"] == "image")));
        let asset_id = text_payload(&evidence)["assets"][0]["assetId"]
            .as_i64()
            .unwrap_or_else(|| panic!("asset payload: {evidence}"));
        asset_ids.push(asset_id);
        let draft = driver.call(
            "lumina_update_chapter_draft",
            json!({
                "chapterId": chapter_id,
                "title": if index == 0 { "雨夜开门" } else { "决定调查" },
                "mainline": if index == 0 { "门在雨夜打开，冲突被引入。" } else { "两人根据线索决定继续调查。" },
                "recap": "固定字幕和固定画面共同构成证据。",
                "outlook": "调查将进入下一段。",
                "highlights": ["固定证据"],
                "questions": ["谁打开了门？"],
                "evidenceAssetIds": [asset_id],
                "draftKey": "blackbox-v1"
            }),
            "draft",
            &mut log,
        );
        assert!(!is_tool_error(&draft), "draft failed: {draft}");
    }

    let finalized = driver.call(
        "lumina_finalize_chapter_task",
        json!({}),
        "finalize",
        &mut log,
    );
    assert!(!is_tool_error(&finalized), "finalize failed: {finalized}");
    assert_eq!(text_payload(&finalized)["status"], "published");
    log.final_status(driver.turn, "succeeded", None);
    assert_jsonl_contract(&log, "succeeded");

    let database = Database::open(&fixture.database_path)
        .unwrap_or_else(|error| panic!("reopen database: {}", error.message));
    let repository = database.repository();
    let chapters = repository
        .list_chapters_by_agent_task(fixture.context.task_id, fixture.context.episode_id)
        .unwrap();
    assert_eq!(chapters.len(), 2);
    assert!(chapters
        .iter()
        .all(|chapter| chapter.status == "ready" && chapter.mainline.is_some()));
    assert_eq!(
        repository
            .list_chapter_assets_by_chapter(chapters[0].id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repository
            .list_chapter_assets_by_chapter(chapters[1].id)
            .unwrap()
            .len(),
        1
    );
    assert!(repository
        .get_agent_task(fixture.context.task_id)
        .unwrap()
        .is_some_and(|task| task.status == "succeeded"));
    assert!(repository
        .list_watch_feed_items_by_episode(fixture.context.episode_id)
        .unwrap()
        .iter()
        .all(|item| item.published_at_ms.is_some()));
    assert_eq!(asset_ids.len(), 2);
}

#[test]
fn chapter_mcp_stdio_records_validation_failure_without_publishing_after_three_invalid_calls() {
    let fixture = fixture("three-failures");
    let mut log = JsonlLog::new(
        &fixture.root,
        "three_failures",
        fixture.context.task_id,
        fixture.context.attempt_id,
    );
    let mut driver = McpDriver::spawn(&fixture, &mut log);
    driver.initialize_and_list(&mut log);
    let chapters = outline(&mut driver, &mut log);
    let chapter_id = chapters[0];
    let mut reasons = Vec::new();
    for retry in 1..=3 {
        let result = driver.call(
            "lumina_update_chapter_draft",
            json!({
                "chapterId": chapter_id,
                "mainline": "这次输入故意引用不存在的证据。",
                "evidenceAssetIds": [999_999],
                "draftKey": "blackbox-invalid"
            }),
            "validation_retry",
            &mut log,
        );
        assert!(
            is_tool_error(&result),
            "retry {retry} unexpectedly succeeded: {result}"
        );
        let reason = result["content"][0]["text"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned();
        assert!(!reason.is_empty());
        reasons.push(reason);
    }
    assert_eq!(reasons.len(), 3);
    log.final_status(
        driver.turn,
        "validation_failure",
        Some("not_published: three invalid MCP tool calls"),
    );
    assert_jsonl_contract(&log, "validation_failure");
    let database = Database::open(&fixture.database_path)
        .unwrap_or_else(|error| panic!("reopen database: {}", error.message));
    let repository = database.repository();
    assert!(repository
        .list_chapter_assets_by_chapter(chapter_id)
        .unwrap()
        .is_empty());
    assert!(repository
        .get_latest_chapter_revision(chapter_id)
        .unwrap()
        .is_none());
    let task = repository
        .get_agent_task(fixture.context.task_id)
        .unwrap()
        .unwrap_or_else(|| panic!("chapter task disappeared"));
    assert_eq!(task.status, "running");
    let chapters = repository
        .list_chapters_by_agent_task(fixture.context.task_id, fixture.context.episode_id)
        .unwrap();
    assert!(chapters.iter().all(|chapter| chapter.status == "draft"));
}

#[test]
fn chapter_mcp_stdio_rejects_foreign_scope_and_unauthorized_tool() {
    let fixture = fixture("scope");
    let mut log = JsonlLog::new(
        &fixture.root,
        "scope",
        fixture.context.task_id,
        fixture.context.attempt_id,
    );
    let mut driver = McpDriver::spawn(&fixture, &mut log);
    driver.initialize_and_list(&mut log);
    let chapters = outline(&mut driver, &mut log);
    let foreign = driver.call(
        "lumina_update_chapter_draft",
        json!({
            "taskId": 999_999,
            "attemptId": 999_999,
            "episodeId": 999_999,
            "chapterId": fixture.foreign_chapter_id,
            "mainline": "越权章节不应写入。",
            "evidenceAssetIds": [1],
            "draftKey": "scope-attack"
        }),
        "scope_rejection",
        &mut log,
    );
    assert!(
        is_tool_error(&foreign),
        "foreign chapter unexpectedly accepted: {foreign}"
    );
    let unauthorized = driver.call(
        "lumina_write_subtitle_track",
        json!({ "lang": "zh-CN", "cues": [] }),
        "scope_rejection",
        &mut log,
    );
    assert!(
        is_tool_error(&unauthorized),
        "unauthorized tool unexpectedly accepted: {unauthorized}"
    );
    log.final_status(
        driver.turn,
        "failed",
        Some("foreign chapter scope or unauthorized tool rejected"),
    );
    assert_jsonl_contract(&log, "failed");
    let database = Database::open(&fixture.database_path)
        .unwrap_or_else(|error| panic!("reopen database: {}", error.message));
    let repository = database.repository();
    assert!(repository
        .list_chapters_by_agent_task(fixture.context.task_id, fixture.context.episode_id)
        .unwrap()[0]
        .mainline
        .is_none());
    assert_eq!(
        repository
            .list_chapter_assets_by_chapter(chapters[0])
            .unwrap()
            .len(),
        0
    );
}
