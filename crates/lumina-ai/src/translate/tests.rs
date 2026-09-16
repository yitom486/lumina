use super::agent::{agent_json, map_agent_error, parse_agent_json, TranslatedBatch};
use super::context::{context_block, extract_reported_names};
use super::*;
use lumina_core::{AgentConversation, AgentTaskError, IsolatedAgentTask};
use serde_json::{json, Value};
use std::sync::Mutex;

#[test]
fn parse_agent_json_strips_fence() {
    let value = parse_agent_json("```json\n{\"cues\":[{\"index\":1,\"text\":\"Hi\"}]}\n```")
        .expect("parse");
    let batch: TranslatedBatch = serde_json::from_value(value).expect("shape");
    assert_eq!(batch.cues.len(), 1);
    assert_eq!(batch.cues[0].text, "Hi");
}

#[test]
fn normalize_zh_uses_explicit_simplified_locale() {
    assert_eq!(
        normalize_translation_language("zh").expect("zh alias"),
        SIMPLIFIED_CHINESE_TOKEN
    );
    assert_eq!(
        normalize_translation_language("zh_CN").expect("zh_CN alias"),
        SIMPLIFIED_CHINESE_TOKEN
    );
}

/// Canned invoker: echoes `TRANSLATED[<text>]` per input cue, preserving
/// count and order across batches (41 cues force two batches).
struct EchoInvoker {
    calls: Mutex<usize>,
}

impl AgentInvoker for EchoInvoker {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        *self.calls.lock().expect("lock") += 1;
        let input: Value =
            serde_json::from_str(task.prompt.rsplit("Input JSON:").next().unwrap_or("{}"))
                .unwrap_or(json!({ "cues": [] }));
        let cues = input
            .get("cues")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let out: Vec<Value> = cues
            .iter()
            .map(|cue| {
                json!({
                    "index": cue.get("index"),
                    "text": format!(
                        "TRANSLATED[{}]",
                        cue.get("text").and_then(|t| t.as_str()).unwrap_or("")
                    ),
                })
            })
            .collect();
        Ok(serde_json::to_string(&json!({ "cues": out })).expect("json"))
    }
}

fn fixture_transcript(cue_count: usize) -> Transcript {
    Transcript {
        source_path: "D:\\video\\demo.mkv".into(),
        choice_id: "cache:subdl:en".into(),
        stream_index: None,
        language: Some("en".into()),
        codec_name: Some("srt".into()),
        cues: (0..cue_count)
            .map(|i| Cue {
                index: i as u32 + 1,
                start_ms: i as u64 * 1000,
                end_ms: i as u64 * 1000 + 800,
                text: format!("line {i}"),
            })
            .collect(),
    }
}

#[test]
fn translate_cues_preserves_timeline_across_batches() {
    let source = fixture_transcript(41);
    let invoker = EchoInvoker {
        calls: Mutex::new(0),
    };
    let mut progress = Vec::new();
    let result = translate_cues(
        &source,
        "zh",
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect("translate");
    let cues = result.cues;
    assert_eq!(cues.len(), 41);
    assert_eq!(*invoker.calls.lock().expect("lock"), 2);
    for (i, cue) in cues.iter().enumerate() {
        assert_eq!(cue.text, format!("TRANSLATED[line {i}]"));
        assert_eq!(cue.start_ms, i as u64 * 1000);
        assert_eq!(cue.end_ms, i as u64 * 1000 + 800);
        assert_eq!(cue.index, i as u32 + 1);
    }
    assert!(progress.iter().any(|message| message.contains('2')));
}

/// Transport flake: first isolated call fails like the harness did on
/// batch 28/33 (JSON-RPC -32602), then behaves like Echo.
struct TransportFlakyInvoker {
    calls: Mutex<usize>,
    echo: EchoInvoker,
}

impl AgentInvoker for TransportFlakyInvoker {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        let mut calls = self.calls.lock().expect("lock");
        *calls += 1;
        if *calls == 1 {
            return Err(AgentTaskError::Failed {
                details: Some("Invalid params".into()),
            });
        }
        drop(calls);
        AgentInvoker::invoke_isolated(&self.echo, task)
    }
}

#[test]
fn transport_failure_fails_fast_without_pool_retry() {
    let source = fixture_transcript(1);
    let invoker = TransportFlakyInvoker {
        calls: Mutex::new(0),
        echo: EchoInvoker {
            calls: Mutex::new(0),
        },
    };
    let mut progress = Vec::new();
    let err = translate_cues(
        &source,
        "zh",
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect_err("transport failure fails fast");
    // Identical replay lives in the workshop pool; a single attempt
    // fails the batch loudly for checkpoint resume.
    assert_eq!(*invoker.calls.lock().expect("lock"), 1);
    assert_eq!(
        err.code,
        lumina_subtitle::error::SubtitleErrorCode::ExportFailed
    );
}

#[test]
fn persistent_transport_failure_fails_fast_without_ai_retry() {
    struct DeadInvoker {
        calls: Mutex<usize>,
    }
    impl AgentInvoker for DeadInvoker {
        fn invoke_isolated(&self, _task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
            *self.calls.lock().expect("lock") += 1;
            Err(AgentTaskError::Failed {
                details: Some("Invalid params".into()),
            })
        }
    }
    let source = fixture_transcript(1);
    let invoker = DeadInvoker {
        calls: Mutex::new(0),
    };
    let mut progress = Vec::new();
    let err = translate_cues(
        &source,
        "zh",
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect_err("persistent failure");
    // No ai-side transport retry (the pool owns identical replay);
    // a single attempt fails the batch loudly for checkpoint resume.
    assert_eq!(*invoker.calls.lock().expect("lock"), 1);
    assert_eq!(
        err.code,
        lumina_subtitle::error::SubtitleErrorCode::ExportFailed
    );
}

#[test]
fn empty_non_json_heals_with_correction_retry() {
    struct EmptyOnceInvoker {
        calls: Mutex<usize>,
        echo: EchoInvoker,
    }

    impl AgentInvoker for EmptyOnceInvoker {
        fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
            let mut calls = self.calls.lock().expect("lock");
            *calls += 1;
            if *calls == 1 {
                return Ok(String::new());
            }
            drop(calls);
            AgentInvoker::invoke_isolated(&self.echo, task)
        }
    }
    let source = fixture_transcript(1);
    let invoker = EmptyOnceInvoker {
        calls: Mutex::new(0),
        echo: EchoInvoker {
            calls: Mutex::new(0),
        },
    };
    let mut progress = Vec::new();
    let result = translate_cues(
        &source,
        "zh",
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect("blank heals");
    assert_eq!(result.cues.len(), 1);
    assert_eq!(*invoker.calls.lock().expect("lock"), 2);
}

#[test]
fn persistent_empty_non_json_fails_after_one_retry() {
    struct EmptyInvoker {
        calls: Mutex<usize>,
    }

    impl AgentInvoker for EmptyInvoker {
        fn invoke_isolated(&self, _task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
            *self.calls.lock().expect("lock") += 1;
            Ok(String::new())
        }
    }
    let source = fixture_transcript(1);
    let invoker = EmptyInvoker {
        calls: Mutex::new(0),
    };
    let mut progress = Vec::new();
    let err = translate_cues(
        &source,
        "zh",
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect_err("persistent blank");
    assert_eq!(*invoker.calls.lock().expect("lock"), 2);
    assert_eq!(
        err.code,
        lumina_subtitle::error::SubtitleErrorCode::ExportFailed
    );
}

#[test]
fn no_output_maps_to_fixed_business_error() {
    let err = map_agent_error(AgentTaskError::NoOutput {
        details: Some("end_turn".into()),
    });
    assert_eq!(
        err.code,
        lumina_subtitle::error::SubtitleErrorCode::NoAgentOutput
    );
    assert_eq!(err.message, "字幕任务未返回有效结果，请重试");
    assert_eq!(err.details.as_deref(), Some("end_turn"));
}

/// Scripted replies for agent_json-level tests: pop one per call.
struct ScriptedInvoker {
    calls: Mutex<usize>,
    prompts: Mutex<Vec<String>>,
    labels: Mutex<Vec<Option<String>>>,
    retry_labels: Mutex<Vec<Option<String>>>,
    replies: Mutex<Vec<String>>,
}

impl AgentInvoker for ScriptedInvoker {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        *self.calls.lock().expect("lock") += 1;
        self.prompts.lock().expect("lock").push(task.prompt);
        self.labels
            .lock()
            .expect("lock")
            .push(task.task_label.clone());
        self.retry_labels
            .lock()
            .expect("lock")
            .push(task.retry_task_label.clone());
        Ok(self.replies.lock().expect("lock").remove(0))
    }
}

fn scripted(replies: Vec<&str>) -> ScriptedInvoker {
    ScriptedInvoker {
        calls: Mutex::new(0),
        prompts: Mutex::new(Vec::new()),
        labels: Mutex::new(Vec::new()),
        retry_labels: Mutex::new(Vec::new()),
        replies: Mutex::new(replies.into_iter().map(str::to_string).collect()),
    }
}

#[test]
fn non_json_heals_with_correction_retry() {
    let invoker = scripted(vec!["definitely not json {{{", "{\"cues\":[]}"]);
    let attempt = AttemptTracker::new("test-job", 0);
    let value = agent_json(
        "codex",
        None,
        None,
        &invoker,
        "do it",
        json!({}),
        "do it",
        json!({}),
        None,
        &attempt,
    )
    .expect("heals");
    assert_eq!(*invoker.calls.lock().expect("lock"), 2);
    let prompts = invoker.prompts.lock().expect("lock");
    assert!(!prompts[0].contains("Correction"));
    assert!(prompts[1].contains("Correction"));
    // Regression lock: the note must precede the payload, otherwise
    // strict parsers (and models) trip over trailing free text.
    let correction_at = prompts[1].find("Correction").expect("note");
    let input_at = prompts[1].find("Input JSON").expect("marker");
    assert!(
        correction_at < input_at,
        "correction must precede Input JSON"
    );
    assert!(value.get("cues").is_some());
    // Labels pin the attempt identity for log correlation.
    let labels = invoker.labels.lock().expect("lock");
    assert_eq!(
        labels[..],
        [
            Some("job=test-job batch=1 content_attempt=1 transport_attempt=1".to_string()),
            Some("job=test-job batch=1 content_attempt=2 transport_attempt=1".to_string()),
        ]
    );
    // Every task precomputes its transport-retry label (T=2) from the
    // same structured source; the pool never parses label strings.
    let retry_labels = invoker.retry_labels.lock().expect("lock");
    assert_eq!(
        retry_labels[..],
        [
            Some("job=test-job batch=1 content_attempt=1 transport_attempt=2".to_string()),
            Some("job=test-job batch=1 content_attempt=2 transport_attempt=2".to_string()),
        ]
    );
}

#[test]
fn persistent_non_json_fails_after_one_retry() {
    let invoker = scripted(vec!["garbage one", "garbage two"]);
    let attempt = AttemptTracker::new("test-job", 0);
    let err = agent_json(
        "codex",
        None,
        None,
        &invoker,
        "do it",
        json!({}),
        "do it",
        json!({}),
        None,
        &attempt,
    )
    .expect_err("persistent garbage");
    assert_eq!(*invoker.calls.lock().expect("lock"), 2);
    assert_eq!(
        err.code,
        lumina_subtitle::error::SubtitleErrorCode::ExportFailed
    );
}

#[test]
fn glossary_retry_bumps_only_content_attempt() {
    let mut source = fixture_transcript(1);
    source.cues[0].text = "Choi Woong is here".into();
    let context = TranslationContext {
        synopsis: None,
        episode_overview: None,
        wiki_episode_plot: None,
        glossary: vec![TranslationGlossaryEntry {
            source: "Choi Woong".into(),
            target: "崔雄".into(),
            verified: true,
        }],
    };
    let invoker = scripted(vec![
        "{\"cues\":[{\"index\":1,\"text\":\"Choi Woong is here\"}],\"glossary\":[]}",
        "{\"cues\":[{\"index\":1,\"text\":\"崔雄在这里\"}],\"glossary\":[]}",
    ]);
    let mut progress = Vec::new();
    let result = translate_cues(
        &source,
        "zh",
        Some(&context),
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect("translate");
    assert_eq!(result.cues[0].text, "崔雄在这里");
    let labels = invoker.labels.lock().expect("lock");
    assert_eq!(
        labels[..],
        [
            Some("job=test-job batch=1 content_attempt=1 transport_attempt=1".to_string()),
            Some("job=test-job batch=1 content_attempt=2 transport_attempt=1".to_string()),
        ]
    );
}

#[test]
fn valid_json_takes_no_extra_retry() {
    let invoker = scripted(vec!["{\"cues\":[]}"]);
    let attempt = AttemptTracker::new("test-job", 0);
    agent_json(
        "codex",
        None,
        None,
        &invoker,
        "do it",
        json!({}),
        "do it",
        json!({}),
        None,
        &attempt,
    )
    .expect("valid");
    assert_eq!(*invoker.calls.lock().expect("lock"), 1);
}

#[test]
fn not_configured_fails_fast_without_retry() {
    struct UnconfiguredInvoker {
        calls: Mutex<usize>,
    }
    impl AgentInvoker for UnconfiguredInvoker {
        fn invoke_isolated(&self, _task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
            *self.calls.lock().expect("lock") += 1;
            Err(AgentTaskError::NotConfigured { details: None })
        }
    }
    let source = fixture_transcript(1);
    let invoker = UnconfiguredInvoker {
        calls: Mutex::new(0),
    };
    let mut progress = Vec::new();
    let err = translate_cues(
        &source,
        "zh",
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect_err("not configured");
    assert_eq!(*invoker.calls.lock().expect("lock"), 1);
    assert_eq!(
        err.code,
        lumina_subtitle::error::SubtitleErrorCode::TranslateNotConfigured
    );
}

#[test]
fn checkpoint_resume_skips_completed_batches() {
    use lumina_core::{BatchCheckpoint, CheckpointBatch};
    use std::collections::BTreeMap;

    struct MemCheckpoint {
        batches: Mutex<BTreeMap<usize, CheckpointBatch>>,
    }
    impl BatchCheckpoint for MemCheckpoint {
        fn load_completed(&self) -> BTreeMap<usize, CheckpointBatch> {
            self.batches.lock().expect("lock").clone()
        }
        fn save_batch(&self, index: usize, batch: &CheckpointBatch) -> Result<(), String> {
            self.batches
                .lock()
                .expect("lock")
                .insert(index, batch.clone());
            Ok(())
        }
        fn clear(&self) -> Result<(), String> {
            self.batches.lock().expect("lock").clear();
            Ok(())
        }
    }

    let source = fixture_transcript(41);
    let store = MemCheckpoint {
        batches: Mutex::new(BTreeMap::new()),
    };
    // Batch 0 finished in a previous (killed) run; batch 1 never ran.
    store
        .save_batch(
            0,
            &CheckpointBatch {
                texts: (1..=40).map(|i| format!("OLD[{i}]")).collect(),
                reported: Vec::new(),
            },
        )
        .expect("seed");
    let invoker = EchoInvoker {
        calls: Mutex::new(0),
    };
    let mut progress = Vec::new();
    let result = translate_cues(
        &source,
        "zh",
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        Some(&store),
        "test-job",
    )
    .expect("resume");
    assert_eq!(result.cues.len(), 41);
    // Only the missing batch hit the model; batch 0 replayed verbatim.
    assert_eq!(*invoker.calls.lock().expect("lock"), 1);
    assert_eq!(result.cues[0].text, "OLD[1]");
    assert_eq!(result.cues[40].text, "TRANSLATED[line 40]");
    // Three progress events prove the resume summary was emitted on top
    // of the two batch completions (replayed + translated).
    assert_eq!(progress.len(), 3);
}

#[test]
fn context_block_pins_names_and_grounds_tone() {
    assert!(context_block(None, false).is_none());
    assert!(context_block(Some(&TranslationContext::default()), false).is_none());
    let context = TranslationContext {
        synopsis: Some("一对前恋人重逢。".into()),
        episode_overview: Some("两人因纪录片项目再次合作。".into()),
        wiki_episode_plot: Some("Ung and Bo-ra meet again after ten years.".into()),
        glossary: vec![
            TranslationGlossaryEntry {
                source: "Choi Woong".into(),
                target: "崔雄".into(),
                verified: true,
            },
            TranslationGlossaryEntry {
                source: "Gu Eun-ho".into(),
                target: "具恩浩".into(),
                verified: false,
            },
        ],
    };
    let (instruction, json) = context_block(Some(&context), false).expect("block");
    assert!(instruction.contains("MUST use the given translation"));
    assert!(instruction.contains("keep the original form unchanged"));
    assert!(instruction.contains("Choi Woong -> 崔雄 (verified; source variant: Choi Ung)"));
    assert!(instruction.contains("Gu Eun-ho -> 具恩浩 (auto)"));
    assert!(instruction.contains("一对前恋人重逢"));
    assert!(instruction.contains("两人因纪录片项目再次合作"));
    assert!(instruction.contains("Ung and Bo-ra meet again after ten years"));
    assert_eq!(json["glossary"][0]["target"], "崔雄");
    assert_eq!(json["episodeOverview"], "两人因纪录片项目再次合作。");
    let (proof_instruction, _) = context_block(Some(&context), true).expect("proofread block");
    assert!(proof_instruction.contains("keep the listed source forms exactly"));
    assert!(!proof_instruction.contains("MUST use the given translation"));
}

#[test]
fn reused_conversation_sends_delta_but_keeps_a_recovery_bootstrap_prompt() {
    struct NeverCalledInvoker;
    impl AgentInvoker for NeverCalledInvoker {
        fn invoke_isolated(&self, _task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
            panic!("the supplied conversation must be used")
        }
    }

    struct CapturingConversation {
        bootstrapped: bool,
        tasks: Vec<IsolatedAgentTask>,
    }
    impl AgentConversation for CapturingConversation {
        fn prompt(&mut self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
            self.bootstrapped = true;
            self.tasks.push(task);
            Ok(r#"{"cues":[{"index":1,"text":"译文"}]}"#.into())
        }

        fn needs_bootstrap(&self) -> bool {
            !self.bootstrapped
        }
    }

    let context = TranslationContext {
        synopsis: Some("A reunion after ten years.".into()),
        ..TranslationContext::default()
    };
    let cue = Cue {
        index: 1,
        start_ms: 0,
        end_ms: 500,
        text: "Hello".into(),
    };
    let invoker = NeverCalledInvoker;
    let mut conversation = CapturingConversation {
        bootstrapped: false,
        tasks: Vec::new(),
    };
    translate_batch(
        std::slice::from_ref(&cue),
        "en",
        "zh-CN",
        Some(&context),
        &[],
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut conversation,
        &AttemptTracker::new("job", 0),
    )
    .expect("first");
    translate_batch(
        std::slice::from_ref(&cue),
        "en",
        "zh-CN",
        Some(&context),
        &[TranslationGlossaryEntry {
            source: "Jang Do-yul".into(),
            target: "张道律".into(),
            verified: false,
        }],
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut conversation,
        &AttemptTracker::new("job", 1),
    )
    .expect("second");

    assert!(conversation.tasks[0]
        .prompt
        .contains("A reunion after ten years."));
    assert!(conversation.tasks[0]
        .prompt
        .contains("source language `en` into target language `zh-CN`"));
    assert!(conversation.tasks[0]
        .prompt
        .contains("`tenth grade` should normally be rendered as `高一`"));
    assert!(!conversation.tasks[1]
        .prompt
        .contains("A reunion after ten years."));
    assert!(conversation.tasks[1]
        .prompt
        .contains("Jang Do-yul -> 张道律 (auto)"));
    assert!(conversation.tasks[1]
        .bootstrap_prompt
        .as_deref()
        .is_some_and(|prompt| prompt.contains("A reunion after ten years.")));
}

#[test]
fn reported_names_survive_the_batch_roundtrip() {
    struct GlossaryInvoker;
    impl AgentInvoker for GlossaryInvoker {
        fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
            assert!(task
                .prompt
                .contains("Choi Woong -> 崔雄 (verified; source variant: Choi Ung)"));
            Ok(serde_json::to_string(&json!({
                "cues": [{"index": 1, "text": "你好，崔雄"}],
                "glossary": [{"source": "Jang Do-yul", "target": "张道律"}]
            }))
            .expect("json"))
        }
    }

    let mut source = fixture_transcript(1);
    source.cues.truncate(1);
    source.cues[0].text = "Jang Do-yul is here".into();
    let context = TranslationContext {
        synopsis: None,
        episode_overview: None,
        wiki_episode_plot: None,
        glossary: vec![TranslationGlossaryEntry {
            source: "Choi Woong".into(),
            target: "崔雄".into(),
            verified: true,
        }],
    };
    let mut progress = Vec::new();
    let result = translate_cues(
        &source,
        "zh",
        Some(&context),
        "codex",
        None,
        None,
        &GlossaryInvoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect("translate");
    assert_eq!(result.cues[0].text, "你好，崔雄");
    assert_eq!(result.reported_names.len(), 1);
    assert_eq!(result.reported_names[0].source, "Jang Do-yul");
    assert_eq!(result.reported_names[0].target, "张道律");
}

struct CapturingInvoker {
    prompts: Mutex<Vec<String>>,
}

impl AgentInvoker for CapturingInvoker {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        let input: Value =
            serde_json::from_str(task.prompt.rsplit("Input JSON:").next().unwrap_or("{}"))
                .unwrap_or(json!({ "cues": [] }));
        let count = input
            .get("cues")
            .and_then(|value| value.as_array())
            .map(|cues| cues.len())
            .unwrap_or(0);
        self.prompts.lock().expect("lock").push(task.prompt);
        let out: Vec<Value> = (0..count)
            .map(|index| json!({"index": index + 1, "text": "clean"}))
            .collect();
        Ok(serde_json::to_string(&json!({ "cues": out })).expect("json"))
    }
}

#[test]
fn proofread_keeps_language_and_pins_glossary_names() {
    let mut source = fixture_transcript(2);
    source.language = Some("en".into());
    source.cues[0].text = "[Music] Choi Ungg is here".into();
    let context = TranslationContext {
        synopsis: None,
        episode_overview: None,
        wiki_episode_plot: None,
        glossary: vec![TranslationGlossaryEntry {
            source: "Choi Ung".into(),
            target: "崔雄".into(),
            verified: true,
        }],
    };
    let invoker = CapturingInvoker {
        prompts: Mutex::new(Vec::new()),
    };
    let mut progress = Vec::new();
    let cues = proofread_cues(
        &source,
        Some(&context),
        true,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect("proofread");
    // Sound tag stripped deterministically, one cue per input preserved.
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].text, "clean");
    let prompts = invoker.prompts.lock().expect("lock");
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains("WITHOUT translating"));
    assert!(prompts[0].contains("Choi Woong -> 崔雄 (verified; source variant: Choi Ung)"));
    assert!(progress.iter().any(|message| message.contains("已完成")));
}

#[test]
fn reported_names_reject_junk_without_failing_cues() {
    let cues = vec![Cue {
        index: 1,
        start_ms: 0,
        end_ms: 800,
        text: "Choi Woong is here".into(),
    }];
    let entries = vec![
        json!({"source": "Choi Woong", "target": "崔雄"}),
        json!({"source": "Ghost", "target": "幽灵"}),
        json!({"source": "Broken"}),
        json!({"source": "", "target": "空"}),
        json!({"source": "NJ", "target": "NJ"}),
    ];
    let names = extract_reported_names(&entries, &cues);
    // Only the mentioned, well-formed, non-identical pair survives;
    // the caller never sees the junk, and cues are unaffected.
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].source, "Choi Woong");
    assert_eq!(names[0].target, "崔雄");
}

fn indexed_cues(pairs: &[(u32, &str)]) -> Vec<Cue> {
    pairs
        .iter()
        .map(|(index, text)| Cue {
            index: *index,
            start_ms: u64::from(*index) * 1000,
            end_ms: u64::from(*index) * 1000 + 800,
            text: (*text).into(),
        })
        .collect()
}

#[test]
fn batch_texts_align_by_returned_index_not_position() {
    let chunk = indexed_cues(&[(7, "a"), (3, "b")]);
    // Reordered model output still lands on the right cues.
    let texts = align_batch_texts(&chunk, vec![(3, "B".into()), (7, "A".into())]).expect("align");
    assert_eq!(texts, vec!["A", "B"]);
    assert!(align_batch_texts(&chunk, vec![(7, "A".into())]).is_err());
    assert!(align_batch_texts(
        &chunk,
        vec![(7, "A".into()), (7, "dup".into()), (3, "B".into())]
    )
    .is_err());
}

#[test]
fn align_retry_heals_dropped_index_once() {
    let chunk = indexed_cues(&[(1, "a"), (2, "b")]);
    let attempt = AttemptTracker::new("test-job", 0);
    let mut calls = 0;
    let mut seen_note = String::new();
    let texts = align_with_one_retry(chunk.as_slice(), vec![(1, "A".into())], &attempt, |note| {
        calls += 1;
        seen_note = note;
        Ok(vec![(1, "A".into()), (2, "B".into())])
    })
    .expect("retry heals");
    assert_eq!(texts, vec!["A", "B"]);
    assert_eq!(calls, 1);
    assert!(
        seen_note.contains('2'),
        "retry note must name the missing cue"
    );
    assert!(
        seen_note.contains("2 条 cue"),
        "retry note must carry the actual batch size"
    );
}

#[test]
fn align_retry_second_answer_stands() {
    let chunk = indexed_cues(&[(1, "a"), (2, "b")]);
    let attempt = AttemptTracker::new("test-job", 0);
    let mut calls = 0;
    let err = align_with_one_retry(chunk.as_slice(), vec![(1, "A".into())], &attempt, |_note| {
        calls += 1;
        Ok(vec![(1, "A".into())])
    })
    .expect_err("still short");
    assert_eq!(calls, 1, "exactly one retry, no loops");
    assert_eq!(
        err.code,
        lumina_subtitle::error::SubtitleErrorCode::ExportFailed
    );
}

#[test]
fn glossary_mismatches_catch_dropped_names() {
    let chunk = indexed_cues(&[(1, "Choi Woong is here"), (2, "Hello")]);
    let glossary = vec![TranslationGlossaryEntry {
        source: "Choi Woong".into(),
        target: "崔雄".into(),
        verified: true,
    }];
    let ok = glossary_mismatches(
        &chunk,
        &["崔雄在这里".to_string(), "你好".to_string()],
        &glossary,
    );
    assert!(ok.is_empty());
    let missed = glossary_mismatches(
        &chunk,
        &["Choi Woong is here".to_string(), "你好".to_string()],
        &glossary,
    );
    assert_eq!(missed.len(), 1);
    assert_eq!(missed[0].source, "Choi Woong");
    assert_eq!(missed[0].expected, "崔雄");
    // Empty glossary never mismatches.
    assert!(glossary_mismatches(&chunk, &["x".to_string()], &[]).is_empty());
}

#[test]
fn missed_names_trigger_one_bounded_retry() {
    struct FlakyInvoker {
        calls: Mutex<usize>,
    }
    impl AgentInvoker for FlakyInvoker {
        fn invoke_isolated(&self, _task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
            let mut calls = self.calls.lock().expect("lock");
            *calls += 1;
            let text = if *calls == 1 { "Choi Woong" } else { "崔雄" };
            Ok(serde_json::to_string(&json!({
                "cues": [{"index": 1, "text": text}],
                "glossary": []
            }))
            .expect("json"))
        }
    }

    let mut source = fixture_transcript(1);
    source.cues.truncate(1);
    source.cues[0].text = "Choi Woong is here".into();
    let context = TranslationContext {
        synopsis: None,
        episode_overview: None,
        wiki_episode_plot: None,
        glossary: vec![TranslationGlossaryEntry {
            source: "Choi Woong".into(),
            target: "崔雄".into(),
            verified: true,
        }],
    };
    let invoker = FlakyInvoker {
        calls: Mutex::new(0),
    };
    let mut progress = Vec::new();
    let result = translate_cues(
        &source,
        "zh",
        Some(&context),
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect("translate");
    assert_eq!(result.cues[0].text, "崔雄");
    assert_eq!(*invoker.calls.lock().expect("lock"), 2);
}

#[test]
fn batch_pool_keeps_order_and_first_error() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    // Reverse-completion sleeps: results must still rejoin 0..8.
    let seen = AtomicUsize::new(0);
    let out = run_batches_in_order(8, |idx| {
        seen.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis((8 - idx) as u64 * 5));
        Ok::<_, String>(idx * 10)
    })
    .expect("pool");
    assert_eq!(out, vec![0, 10, 20, 30, 40, 50, 60, 70]);
    assert_eq!(seen.load(Ordering::SeqCst), 8);

    // Zero jobs short-circuit.
    let empty: Vec<u32> = run_batches_in_order(0, |_| Ok::<_, String>(0)).expect("empty");
    assert!(empty.is_empty());

    // First error wins deterministically.
    let err = run_batches_in_order(4, |idx| {
        if idx == 2 {
            Err("boom".to_string())
        } else {
            Ok(idx)
        }
    })
    .expect_err("failing pool");
    assert_eq!(err, "boom");
}

#[test]
fn translate_cues_rejects_empty_source() {
    let source = fixture_transcript(0);
    let invoker = EchoInvoker {
        calls: Mutex::new(0),
    };
    let mut progress = Vec::new();
    let err = translate_cues(
        &source,
        "zh",
        None,
        "codex",
        None,
        None,
        &invoker,
        &mut |update: ProgressUpdate| progress.push(update.message),
        None,
        "test-job",
    )
    .expect_err("empty source");
    assert_eq!(err.message, "无法保存字幕文件");
}
