//! AcpService — optional on-demand ACP session over stdio JSON-RPC.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::acp::error::AcpError;
use crate::acp::model::{AcpEvent, AcpStatus, AgentProfileInput};
use crate::acp::paths::status_from_store;
use crate::acp::profile::{resolve_launch, AgentKind, AgentProfile, ProfileStore};
use crate::acp::protocol::{
    encode_line, extract_agent_text, initialize_params, is_error_response, parse_session_id,
    request, session_new_params, session_prompt_params,
};

pub struct AcpService {
    busy: AtomicBool,
    cancel: AtomicBool,
    child: Mutex<Option<std::process::Child>>,
    profiles: ProfileStore,
}

impl AcpService {
    pub fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            child: Mutex::new(None),
            profiles: ProfileStore::new(),
        }
    }

    pub fn status(&self) -> AcpStatus {
        status_from_store(&self.profiles)
    }

    pub fn set_active_profile(&self, id: &str) -> Result<(), AcpError> {
        self.profiles.set_active(id)
    }

    pub fn upsert_profile(&self, input: AgentProfileInput) -> Result<AgentProfile, AcpError> {
        self.profiles.upsert(AgentProfile {
            id: input.id,
            name: input.name,
            kind: input.kind,
            command: input.command,
            args: input.args,
            env: input.env,
        })
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.child.lock() {
            if let Some(child) = slot.as_mut() {
                let _ = child.kill();
            }
        }
    }

    /// Minimal ACP prompt session using the active (or overridden) profile.
    pub fn prompt<F>(
        &self,
        text: impl AsRef<str>,
        cwd: Option<String>,
        profile_id: Option<String>,
        mut on_event: F,
    ) -> Result<String, AcpError>
    where
        F: FnMut(AcpEvent),
    {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AcpError::busy());
        }
        self.cancel.store(false, Ordering::SeqCst);

        let outcome =
            self.run_prompt_inner(text.as_ref(), cwd.as_deref(), profile_id.as_deref(), &mut on_event);

        self.busy.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.child.lock() {
            if let Some(mut child) = slot.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        match &outcome {
            Ok(text) => on_event(AcpEvent::Finished {
                text: text.clone(),
            }),
            Err(error) if error.code == crate::acp::AcpErrorCode::Cancelled => {
                on_event(AcpEvent::Failed {
                    code: "Cancelled".into(),
                    message: error.message.clone(),
                });
            }
            Err(error) => on_event(AcpEvent::Failed {
                code: format!("{:?}", error.code),
                message: error.message.clone(),
            }),
        }

        outcome
    }

    fn run_prompt_inner(
        &self,
        prompt_text: &str,
        cwd: Option<&str>,
        profile_id: Option<&str>,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<String, AcpError> {
        let prompt_text = prompt_text.trim();
        if prompt_text.is_empty() {
            return Err(AcpError::bad_request("提问内容不能为空"));
        }

        if let Some(id) = profile_id {
            self.profiles.set_active(id)?;
        }
        let profile = self.profiles.active_profile()?;
        if profile.command.trim().is_empty() {
            return Err(AcpError::not_configured(Some(
                "active profile has empty command",
            )));
        }

        let launch = resolve_launch(&profile)?;
        on_event(AcpEvent::Started);
        on_event(AcpEvent::Progress {
            message: format!("正在启动 {}…", launch.display_name),
        });

        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &launch.env {
            command.env(key, value);
        }
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }

        let mut child = command
            .spawn()
            .map_err(|error| AcpError::spawn_failed(Some(&error.to_string())))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AcpError::spawn_failed(Some("stdin pipe missing")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AcpError::spawn_failed(Some("stdout pipe missing")))?;

        {
            let mut slot = self
                .child
                .lock()
                .map_err(|_| AcpError::internal(Some("ACP child mutex poisoned")))?;
            *slot = Some(child);
        }

        let write_req =
            |stdin: &mut std::process::ChildStdin, id: u64, method: &str, params: Value| {
                let line = encode_line(&request(id, method, params))?;
                writeln!(stdin, "{line}").map_err(|error| {
                    tracing::warn!(%error, "ACP write failed");
                    AcpError::protocol(Some(&format!("write ACP request: {error}")))
                })?;
                stdin.flush().map_err(|error| {
                    tracing::warn!(%error, "ACP flush failed");
                    AcpError::protocol(Some(&format!("flush ACP stdin: {error}")))
                })?;
                Ok::<(), AcpError>(())
            };

        let mut reader = BufReader::new(stdout);
        let mut line_buf = String::new();

        let mut read_until_id = |target_id: u64| -> Result<Value, AcpError> {
            loop {
                if self.cancel.load(Ordering::SeqCst) {
                    return Err(AcpError::cancelled());
                }
                line_buf.clear();
                let bytes = reader.read_line(&mut line_buf).map_err(|error| {
                    tracing::warn!(%error, "ACP read failed");
                    AcpError::protocol(Some(&format!("read ACP stdout: {error}")))
                })?;
                if bytes == 0 {
                    return Err(AcpError::protocol(Some("EOF on stdout")));
                }
                let trimmed = line_buf.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(trimmed).map_err(|error| {
                    tracing::warn!(%error, line = trimmed, "ACP JSON parse failed");
                    AcpError::protocol(Some(&format!("parse ACP JSON: {error}")))
                })?;

                if value.get("id").and_then(|v| v.as_u64()) == Some(target_id) {
                    if let Some(msg) = is_error_response(&value) {
                        return Err(AcpError::protocol(Some(&msg)));
                    }
                    return Ok(value);
                }
            }
        };

        on_event(AcpEvent::Progress {
            message: "正在初始化会话…".into(),
        });
        write_req(&mut stdin, 1, "initialize", initialize_params())?;
        let _init = read_until_id(1)?;

        on_event(AcpEvent::Progress {
            message: "正在创建会话…".into(),
        });
        write_req(
            &mut stdin,
            2,
            "session/new",
            session_new_params(cwd),
        )?;
        let session_resp = read_until_id(2)?;
        let session_id = parse_session_id(&session_resp).ok_or_else(|| {
            AcpError::protocol(Some(&format!("missing sessionId: {session_resp}")))
        })?;

        on_event(AcpEvent::Progress {
            message: "正在发送问题…".into(),
        });
        write_req(
            &mut stdin,
            3,
            "session/prompt",
            session_prompt_params(&session_id, prompt_text),
        )?;

        let mut collected = String::new();
        let deadline = Instant::now() + Duration::from_secs(600);
        let empty_hint = match profile.kind {
            AgentKind::Codex => {
                "（会话结束，未解析到文本回复；请确认 Codex 已登录，且模型走 Responses API）"
            }
            _ => "（会话结束，未解析到文本回复；请确认该 ACP Agent 可用）",
        };

        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(AcpError::cancelled());
            }
            if Instant::now() > deadline {
                return Err(AcpError::protocol(Some("ACP wait timed out")));
            }

            line_buf.clear();
            let bytes = reader.read_line(&mut line_buf).map_err(|error| {
                tracing::warn!(%error, "ACP read failed");
                AcpError::protocol(Some(&format!("read ACP stdout: {error}")))
            })?;
            if bytes == 0 {
                break;
            }
            let trimmed = line_buf.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(error) => {
                    tracing::warn!(%error, line = trimmed, "skip non-json ACP line");
                    continue;
                }
            };

            if let Some(text) = extract_agent_text(&value) {
                if !text.is_empty() {
                    collected.push_str(&text);
                    on_event(AcpEvent::AgentMessage { text });
                }
            }

            if value.get("id").and_then(|v| v.as_u64()) == Some(3) {
                if let Some(msg) = is_error_response(&value) {
                    return Err(AcpError::protocol(Some(&msg)));
                }
                break;
            }
        }

        if collected.is_empty() {
            collected = empty_hint.into();
        }
        Ok(collected)
    }
}

impl Default for AcpService {
    fn default() -> Self {
        Self::new()
    }
}
