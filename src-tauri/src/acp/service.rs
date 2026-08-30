//! AcpService — optional on-demand ACP Client over stdio JSON-RPC.
//!
//! Implements baseline Client→Agent: initialize, authenticate (if required),
//! session/new, session/prompt, session/cancel, session/close (when advertised).
//! Handles Agent→Client: session/update (+ variants), session/request_permission,
//! and rejects fs/terminal/elicitation with protocol errors.
//! Keeps one live session for multi-turn until close/cancel-kill.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::acp::error::AcpError;
use crate::acp::model::{AcpEvent, AcpStatus, AgentProfileInput};
use crate::acp::paths::status_from_store;
use crate::acp::profile::{resolve_launch, AgentKind, AgentProfile, ProfileStore};
use crate::acp::protocol::{
    authenticate_params, classify_inbound, encode_line, extract_agent_text, extract_plan_summary,
    extract_thought_text, extract_tool_call, handle_agent_request, initialize_params,
    is_error_response, notification, parse_initialize_result, parse_session_id, parse_stop_reason,
    request, session_cancel_params, session_close_params, session_new_params,
    session_prompt_params, Inbound, InitializeResult,
};

struct LiveSession {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<std::process::ChildStdout>,
    session_id: String,
    next_id: u64,
    init: InitializeResult,
    profile_kind: AgentKind,
}

pub struct AcpService {
    busy: AtomicBool,
    cancel: AtomicBool,
    session: Mutex<Option<LiveSession>>,
    profiles: ProfileStore,
}

impl AcpService {
    pub fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            session: Mutex::new(None),
            profiles: ProfileStore::new(),
        }
    }

    pub fn status(&self) -> AcpStatus {
        let mut status = status_from_store(&self.profiles);
        status.busy = self.busy.load(Ordering::SeqCst);
        status.session_active = self
            .session
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false);
        status
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

    /// Soft-cancel: set flag + send `session/cancel`. Kill is last-resort after timeout.
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.session.lock() {
            if let Some(session) = guard.as_mut() {
                let _ = Self::write_notification(
                    &mut session.stdin,
                    "session/cancel",
                    session_cancel_params(&session.session_id),
                );
            }
        }
    }

    /// Close live session (`session/close` when supported) and kill process.
    pub fn close_session(&self) -> Result<(), AcpError> {
        self.cancel.store(true, Ordering::SeqCst);
        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        if let Some(mut session) = guard.take() {
            if session.init.supports_session_close {
                let id = session.next_id;
                session.next_id += 1;
                let _ = Self::write_request(
                    &mut session.stdin,
                    id,
                    "session/close",
                    session_close_params(&session.session_id),
                );
                // Best-effort brief drain; then kill.
                let _ = Self::read_until_id_raw(&mut session, id, Duration::from_secs(2), &AtomicBool::new(false), &mut |_| {});
            }
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
        self.cancel.store(false, Ordering::SeqCst);
        Ok(())
    }

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

        match &outcome {
            Ok((text, stop_reason)) => on_event(AcpEvent::Finished {
                text: text.clone(),
                stop_reason: stop_reason.clone(),
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

        outcome.map(|(text, _)| text)
    }

    fn run_prompt_inner(
        &self,
        prompt_text: &str,
        cwd: Option<&str>,
        profile_id: Option<&str>,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(String, Option<String>), AcpError> {
        let prompt_text = prompt_text.trim();
        if prompt_text.is_empty() {
            return Err(AcpError::bad_request("提问内容不能为空"));
        }

        if let Some(id) = profile_id {
            self.profiles.set_active(id)?;
        }

        // Ensure live session (reuse when possible).
        {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
            if guard.is_none() {
                *guard = Some(self.spawn_session(cwd, on_event)?);
            }
        }

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AcpError::internal(Some("ACP session missing after spawn")))?;

        on_event(AcpEvent::Progress {
            message: "正在发送问题…".into(),
        });

        let prompt_id = session.next_id;
        session.next_id += 1;
        Self::write_request(
            &mut session.stdin,
            prompt_id,
            "session/prompt",
            session_prompt_params(&session.session_id, prompt_text),
        )?;

        let empty_hint = match session.profile_kind {
            AgentKind::Codex => {
                "（会话结束，未解析到文本回复；请确认 Codex 已登录，且模型走 Responses API）"
            }
            _ => "（会话结束，未解析到文本回复；请确认该 ACP Agent 可用）",
        };

        let mut collected = String::new();
        let mut on_event_collect = |ev: AcpEvent| {
            if let AcpEvent::AgentMessage { text } = &ev {
                collected.push_str(text);
            }
            on_event(ev);
        };

        let deadline = Instant::now() + Duration::from_secs(600);
        let mut cancel_sent = false;
        let mut cancel_at: Option<Instant> = None;

        loop {
            if self.cancel.load(Ordering::SeqCst) && !cancel_sent {
                let _ = Self::write_notification(
                    &mut session.stdin,
                    "session/cancel",
                    session_cancel_params(&session.session_id),
                );
                cancel_sent = true;
                cancel_at = Some(Instant::now());
            }

            if Instant::now() > deadline {
                let _ = session.child.kill();
                let _ = guard.take();
                return Err(AcpError::protocol(Some("ACP wait timed out")));
            }

            if let Some(at) = cancel_at {
                if Instant::now().duration_since(at) > Duration::from_secs(8) {
                    let _ = session.child.kill();
                    let _ = guard.take();
                    return Err(AcpError::cancelled());
                }
            }

            match Self::read_one(
                session,
                Duration::from_millis(250),
                &self.cancel,
                &mut on_event_collect,
            )? {
                ReadOne::TimedOut => continue,
                ReadOne::Eof => {
                    let _ = guard.take();
                    if self.cancel.load(Ordering::SeqCst) {
                        return Err(AcpError::cancelled());
                    }
                    break;
                }
                ReadOne::Response { id, value } if id == prompt_id => {
                    if let Some(msg) = is_error_response(&value) {
                        tracing::warn!(%msg, "ACP prompt error response");
                        return Err(AcpError::protocol(Some(&msg)));
                    }
                    let stop = parse_stop_reason(&value);
                    if stop.as_deref() == Some("cancelled") || self.cancel.load(Ordering::SeqCst)
                    {
                        return Err(AcpError::cancelled());
                    }
                    if collected.is_empty() {
                        collected = empty_hint.into();
                    }
                    return Ok((collected, stop));
                }
                ReadOne::Response { .. } => continue,
            }
        }

        if self.cancel.load(Ordering::SeqCst) {
            let _ = guard.take();
            return Err(AcpError::cancelled());
        }
        if collected.is_empty() {
            collected = empty_hint.into();
        }
        Ok((collected, Some("end_turn".into())))
    }

    fn spawn_session(
        &self,
        cwd: Option<&str>,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<LiveSession, AcpError> {
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
            .map_err(|error| {
                tracing::warn!(%error, "ACP spawn failed");
                AcpError::spawn_failed(Some(&error.to_string()))
            })?;

        // Drain stderr so the pipe never blocks the agent.
        if let Some(stderr) = child.stderr.take() {
            thread::spawn(move || {
                let mut buf = String::new();
                let mut reader = BufReader::new(stderr);
                while let Ok(n) = reader.read_line(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let line = buf.trim();
                    if !line.is_empty() {
                        tracing::warn!(stderr = line, "ACP agent stderr");
                    }
                    buf.clear();
                }
            });
        }

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AcpError::spawn_failed(Some("stdin pipe missing")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AcpError::spawn_failed(Some("stdout pipe missing")))?;
        let reader = BufReader::new(stdout);

        let mut session = LiveSession {
            child,
            stdin,
            reader,
            session_id: String::new(),
            next_id: 1,
            init: InitializeResult::default(),
            profile_kind: profile.kind,
        };

        on_event(AcpEvent::Progress {
            message: "正在初始化会话…".into(),
        });
        let init_id = session.next_id;
        session.next_id += 1;
        Self::write_request(
            &mut session.stdin,
            init_id,
            "initialize",
            initialize_params(),
        )?;
        let init_resp = Self::read_until_id_raw(
            &mut session,
            init_id,
            Duration::from_secs(60),
            &self.cancel,
            on_event,
        )?;
        let init = parse_initialize_result(&init_resp);
        if let Some(ver) = init.protocol_version {
            if ver != 1 {
                tracing::warn!(ver, "ACP protocolVersion is not 1; continuing");
            }
        }
        if let Some(name) = &init.agent_name {
            on_event(AcpEvent::Progress {
                message: format!("已连接 {name}"),
            });
        }

        // Authenticate when Agent requires it (first advertised method).
        if !init.auth_methods.is_empty() {
            let method = &init.auth_methods[0];
            on_event(AcpEvent::Progress {
                message: format!("正在认证（{}）…", method.name),
            });
            let auth_id = session.next_id;
            session.next_id += 1;
            Self::write_request(
                &mut session.stdin,
                auth_id,
                "authenticate",
                authenticate_params(&method.id),
            )?;
            let auth_resp = Self::read_until_id_raw(
                &mut session,
                auth_id,
                Duration::from_secs(120),
                &self.cancel,
                on_event,
            )?;
            if let Some(msg) = is_error_response(&auth_resp) {
                let _ = session.child.kill();
                return Err(AcpError::protocol(Some(&format!("authenticate failed: {msg}"))));
            }
        }

        session.init = init;

        on_event(AcpEvent::Progress {
            message: "正在创建会话…".into(),
        });
        let new_id = session.next_id;
        session.next_id += 1;
        Self::write_request(
            &mut session.stdin,
            new_id,
            "session/new",
            session_new_params(cwd),
        )?;
        let session_resp = Self::read_until_id_raw(
            &mut session,
            new_id,
            Duration::from_secs(60),
            &self.cancel,
            on_event,
        )?;
        let session_id = parse_session_id(&session_resp).ok_or_else(|| {
            AcpError::protocol(Some(&format!("missing sessionId: {session_resp}")))
        })?;
        session.session_id = session_id;
        Ok(session)
    }

    fn write_request(
        stdin: &mut ChildStdin,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<(), AcpError> {
        let line = encode_line(&request(id, method, params))?;
        writeln!(stdin, "{line}").map_err(|error| {
            tracing::warn!(%error, method, "ACP write failed");
            AcpError::protocol(Some(&format!("write ACP request: {error}")))
        })?;
        stdin.flush().map_err(|error| {
            tracing::warn!(%error, "ACP flush failed");
            AcpError::protocol(Some(&format!("flush ACP stdin: {error}")))
        })?;
        Ok(())
    }

    fn write_notification(
        stdin: &mut ChildStdin,
        method: &str,
        params: Value,
    ) -> Result<(), AcpError> {
        let line = encode_line(&notification(method, params))?;
        writeln!(stdin, "{line}").map_err(|error| {
            tracing::warn!(%error, method, "ACP notification write failed");
            AcpError::protocol(Some(&format!("write ACP notification: {error}")))
        })?;
        stdin.flush().map_err(|error| {
            AcpError::protocol(Some(&format!("flush ACP stdin: {error}")))
        })?;
        Ok(())
    }

    fn write_raw(stdin: &mut ChildStdin, value: &Value) -> Result<(), AcpError> {
        let line = encode_line(value)?;
        writeln!(stdin, "{line}").map_err(|error| {
            AcpError::protocol(Some(&format!("write ACP response: {error}")))
        })?;
        stdin.flush().map_err(|error| {
            AcpError::protocol(Some(&format!("flush ACP stdin: {error}")))
        })?;
        Ok(())
    }

    fn emit_session_update(value: &Value, on_event: &mut dyn FnMut(AcpEvent)) {
        if let Some(text) = extract_agent_text(value) {
            if !text.is_empty() {
                on_event(AcpEvent::AgentMessage { text });
            }
        }
        if let Some(text) = extract_thought_text(value) {
            if !text.is_empty() {
                on_event(AcpEvent::AgentThought { text });
            }
        }
        if let Some(tool) = extract_tool_call(value) {
            if tool.update_kind == "tool_call" {
                on_event(AcpEvent::ToolCall {
                    tool_call_id: tool.tool_call_id,
                    title: tool.title,
                    kind: tool.kind,
                    status: tool.status,
                });
            } else {
                on_event(AcpEvent::ToolCallUpdate {
                    tool_call_id: tool.tool_call_id,
                    status: tool.status,
                    title: tool.title,
                });
            }
        }
        if let Some(text) = extract_plan_summary(value) {
            on_event(AcpEvent::Plan { text });
        }
    }

    fn handle_inbound_side_effects(
        session: &mut LiveSession,
        inbound: Inbound,
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<Option<(u64, Value)>, AcpError> {
        match inbound {
            Inbound::Response { id, value } => Ok(Some((id, value))),
            Inbound::Notification { method, params } => {
                if method == "session/update" {
                    let wrapped = serde_json::json!({
                        "method": "session/update",
                        "params": params,
                    });
                    Self::emit_session_update(&wrapped, on_event);
                } else {
                    tracing::debug!(%method, "ignored ACP notification");
                }
                Ok(None)
            }
            Inbound::AgentRequest { id, method, params } => {
                let canceling = cancel.load(Ordering::SeqCst);
                if method == "session/request_permission" {
                    let tool_id = params
                        .pointer("/toolCall/toolCallId")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let decision = if canceling {
                        "cancelled"
                    } else {
                        "auto"
                    };
                    on_event(AcpEvent::PermissionResolved {
                        tool_call_id: tool_id,
                        decision: decision.into(),
                    });
                }
                let response = handle_agent_request(&method, id, &params, canceling);
                Self::write_raw(&mut session.stdin, &response)?;
                Ok(None)
            }
            Inbound::Other(value) => {
                tracing::debug!(%value, "ignored ACP message");
                Ok(None)
            }
        }
    }

    fn read_until_id_raw(
        session: &mut LiveSession,
        target_id: u64,
        timeout: Duration,
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<Value, AcpError> {
        let deadline = Instant::now() + timeout;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(AcpError::cancelled());
            }
            if Instant::now() > deadline {
                return Err(AcpError::protocol(Some("ACP read timed out")));
            }
            match Self::read_one(session, Duration::from_millis(200), cancel, on_event)? {
                ReadOne::TimedOut => continue,
                ReadOne::Eof => return Err(AcpError::protocol(Some("EOF on stdout"))),
                ReadOne::Response { id, value } if id == target_id => {
                    if let Some(msg) = is_error_response(&value) {
                        return Err(AcpError::protocol(Some(&msg)));
                    }
                    return Ok(value);
                }
                ReadOne::Response { .. } => continue,
            }
        }
    }

    fn read_one(
        session: &mut LiveSession,
        wait: Duration,
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<ReadOne, AcpError> {
        // Non-blocking-ish: use set_read_timeout when available.
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // fall through to blocking read with short patience — Windows path below
            let _ = wait;
        }
        let _ = wait;

        // Blocking read_line; cancel is checked between lines by callers using short loops.
        // To honor timeout better on Windows, we rely on caller slice loops + cancel.
        let mut line_buf = String::new();
        // Peek with a short sleep if no data — actually BufReader doesn't support easily.
        // Use read_line blocking; outer loop checks cancel/deadline.
        let bytes = session.reader.read_line(&mut line_buf).map_err(|error| {
            tracing::warn!(%error, "ACP read failed");
            AcpError::protocol(Some(&format!("read ACP stdout: {error}")))
        })?;
        if bytes == 0 {
            return Ok(ReadOne::Eof);
        }
        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            return Ok(ReadOne::TimedOut);
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|error| {
            tracing::warn!(%error, line = trimmed, "ACP JSON parse failed");
            AcpError::protocol(Some(&format!("parse ACP JSON: {error}")))
        })?;

        match Self::handle_inbound_side_effects(
            session,
            classify_inbound(value),
            cancel,
            on_event,
        )? {
            Some((id, value)) => Ok(ReadOne::Response { id, value }),
            None => Ok(ReadOne::TimedOut),
        }
    }
}

enum ReadOne {
    TimedOut,
    Eof,
    Response { id: u64, value: Value },
}

impl Default for AcpService {
    fn default() -> Self {
        Self::new()
    }
}
