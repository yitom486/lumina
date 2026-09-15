//! AcpService — optional on-demand ACP Client over stdio JSON-RPC.
//!
//! Baseline Client→Agent: initialize, authenticate, session/new|prompt|cancel|close.
//! Agent→Client: session/update, session/request_permission, fs/*, terminal/*.
//!
//! Facade only (no behavior change). Lifecycle lives in `super::lifecycle`,
//! line IO in `super::io`, inbound handling in `super::inbound`, isolated
//! workshop tasks in `crate::jobs::isolated`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use crate::agent::profile::{
    prepare_profiles, resolve_active_profile, AgentKind, PreparedProfiles,
};
use crate::agent::status::status_from_profiles;
use crate::agent::workspace::resolve_session_cwd;
use crate::domain::context::VideoPromptContext;
use crate::domain::environment::session_env;
use crate::domain::model::{
    AcpEvent, AcpModelDiscoveryResult, AcpSessionModelOptions, AcpSessionModelSelection, AcpStatus,
    AgentProfilesHint, SavedSessionHint,
};
use crate::domain::settings::{AcpClientSettings, PermissionMode};
use crate::error::AcpError;
use crate::jobs::collector::AgentReplyCollector;
use crate::runtime::host::AcpHost;
use crate::wire::codec::is_error_response;
use crate::wire::session::{parse_stop_reason, session_cancel_params};

pub struct AcpService {
    pub(crate) busy: AtomicBool,
    pub(crate) cancel: AtomicBool,
    pub(crate) session: Mutex<Option<crate::runtime::lifecycle::LiveSession>>,
    pub(crate) host: AcpHost,
    pub(crate) permission_mode: Mutex<PermissionMode>,
    pub(crate) permission_replies: Mutex<Option<mpsc::Sender<Option<String>>>>,
    pub(crate) permission_seq: AtomicU64,
    pub(crate) tool_access_enabled: AtomicBool,
    pub(crate) next_session_model_selection: Mutex<Option<AcpSessionModelSelection>>,
}

impl AcpService {
    pub fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            session: Mutex::new(None),
            host: AcpHost::new(),
            permission_mode: Mutex::new(PermissionMode::Auto),
            permission_replies: Mutex::new(None),
            permission_seq: AtomicU64::new(1),
            tool_access_enabled: AtomicBool::new(true),
            next_session_model_selection: Mutex::new(None),
        }
    }

    pub fn respond_permission(
        &self,
        _request_id: &str,
        option_id: Option<String>,
    ) -> Result<(), AcpError> {
        let mut guard = self
            .permission_replies
            .lock()
            .map_err(|_| AcpError::internal(Some("permission mutex poisoned")))?;
        if let Some(tx) = guard.take() {
            let _ = tx.send(option_id);
            Ok(())
        } else {
            Err(AcpError::protocol(Some("no pending permission request")))
        }
    }

    pub fn status(&self, profiles: &AgentProfilesHint) -> AcpStatus {
        let mut status = status_from_profiles(profiles);
        status.busy = self.busy.load(Ordering::SeqCst);
        if let Ok(guard) = self.session.lock() {
            status.session_active = guard.is_some();
            status.session_model_options =
                guard.as_ref().map(|session| session.model_options.clone());
        }
        status
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// Soft-cancel: set flag + send `session/cancel`. Kill is last-resort after timeout.
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.session.lock() {
            if let Some(session) = guard.as_mut() {
                let _ = crate::runtime::io::write_notification(
                    &mut session.stdin,
                    "session/cancel",
                    session_cancel_params(&session.session_id),
                );
            }
        }
    }

    /// Close live session (`session/close` when supported) and kill process.
    /// Chat snapshot warm state is owned by the app adapter (M5).
    pub fn close_session(&self) -> Result<(), AcpError> {
        self.cancel.store(true, Ordering::SeqCst);
        self.drop_live_session(true);
        self.cancel.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// App exit: kill agent/terminal children without waiting on graceful handshakes.
    pub fn close_session_for_shutdown(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.drop_live_session(false);
    }

    /// Warm up Agent process + session without sending a prompt (user opened chat tab).
    pub fn connect<F>(
        &self,
        cwd: Option<String>,
        profile_id: Option<String>,
        saved_session: Option<SavedSessionHint>,
        client_settings: AcpClientSettings,
        profiles: AgentProfilesHint,
        mut on_event: F,
    ) -> Result<(), AcpError>
    where
        F: FnMut(AcpEvent),
    {
        if self.is_busy() {
            return Err(AcpError::busy());
        }

        self.cancel.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.permission_mode.lock() {
            *guard = client_settings.permission_mode;
        }

        let prepared = prepare_profiles(&profiles);
        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;

        if guard.is_some() {
            // Keep the existing-session capability sync inline (was `sync_mcp_capabilities`);
            // snapshot IO now goes through the app-provided environment.
            let env = session_env()?;
            let workspace = resolve_session_cwd(cwd.as_deref())?;
            env.sync_snapshot(
                &env.snapshot_path(&workspace),
                client_settings.vision_capable,
            )
            .map_err(|error| AcpError::internal(Some(&error)))?;
            if let Some(session) = guard.as_mut() {
                if let Some(selection) = client_settings.model_selection() {
                    let _ = self.apply_model_selection(session, &selection, &mut on_event);
                }
            }
            on_event(AcpEvent::Progress {
                message: "Agent 已连接".into(),
            });
            return Ok(());
        }

        match self.spawn_session(
            cwd.as_deref(),
            saved_session.as_ref(),
            &prepared,
            profile_id.as_deref(),
            client_settings.vision_capable,
            &mut on_event,
        ) {
            Ok(mut session) => {
                if let Some(selection) = client_settings.model_selection() {
                    self.apply_model_selection(&mut session, &selection, &mut on_event)?;
                }
                *guard = Some(session);
                on_event(AcpEvent::Progress {
                    message: "Agent 已就绪".into(),
                });
                Ok(())
            }
            Err(error) => {
                drop(guard);
                self.drop_live_session(true);
                Err(error)
            }
        }
    }

    /// Start a fresh Agent session for chat UI: clear resume hint, rotate `session/new`
    /// on an existing process when possible (Cursor-style), otherwise connect.
    pub fn new_chat<F>(
        &self,
        cwd: Option<String>,
        profile_id: Option<String>,
        client_settings: AcpClientSettings,
        profiles: AgentProfilesHint,
        mut on_event: F,
    ) -> Result<(), AcpError>
    where
        F: FnMut(AcpEvent),
    {
        if self.is_busy() {
            return Err(AcpError::busy());
        }

        self.cancel.store(false, Ordering::SeqCst);
        // Chat snapshot warm state is owned/reset by the app adapter (M5).
        if let Ok(mut guard) = self.permission_mode.lock() {
            *guard = client_settings.permission_mode;
        }

        let prepared = prepare_profiles(&profiles);
        let workspace = resolve_session_cwd(cwd.as_deref())?;
        let cwd_string = workspace.to_string_lossy().into_owned();
        let profile = resolve_active_profile(&prepared, profile_id.as_deref())?;

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;

        if let Some(session) = guard.as_mut() {
            on_event(AcpEvent::Progress {
                message: "正在开始新对话…".into(),
            });
            self.host.set_workspace(workspace);
            self.host.release_all();

            if session.init.supports_session_close {
                if let Err(error) = Self::close_agent_session(self, session, &mut on_event) {
                    tracing::warn!(%error, "session/close failed during new chat; respawning agent");
                    drop(guard);
                    self.drop_live_session(true);
                    return self.connect(
                        Some(cwd_string),
                        profile_id,
                        None,
                        client_settings,
                        profiles,
                        on_event,
                    );
                }
            }

            let new_session_id = self.create_new_session(
                session,
                &cwd_string,
                &profile.id,
                client_settings.vision_capable,
                &mut on_event,
            )?;
            session.session_id = new_session_id;

            if let Some(selection) = client_settings.model_selection() {
                self.apply_model_selection(session, &selection, &mut on_event)?;
            } else if let Ok(selection_guard) = self.next_session_model_selection.lock() {
                if let Some(selection) = selection_guard.clone() {
                    self.apply_model_selection(session, &selection, &mut on_event)?;
                }
            }

            on_event(AcpEvent::Progress {
                message: "新对话已就绪".into(),
            });
            Ok(())
        } else {
            drop(guard);
            self.connect(cwd, profile_id, None, client_settings, profiles, on_event)
        }
    }

    /// Apply model / reasoning overrides to the live session without rotating it.
    pub fn set_session_model<F>(
        &self,
        model_id: Option<String>,
        reasoning_effort: Option<String>,
        mut on_event: F,
    ) -> Result<AcpSessionModelOptions, AcpError>
    where
        F: FnMut(AcpEvent),
    {
        if self.is_busy() {
            return Err(AcpError::busy());
        }

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AcpError::protocol(Some("no active agent session")))?;

        if let Some(model_id) = model_id.filter(|value| !value.trim().is_empty()) {
            Self::set_session_config_option(
                session,
                "model",
                model_id.trim(),
                self,
                &mut on_event,
            )?;
            session.model_options.current_model_id = Some(model_id.trim().to_string());
        }

        if let Some(reasoning_effort) = reasoning_effort.filter(|value| !value.trim().is_empty()) {
            Self::set_session_config_option(
                session,
                "reasoning_effort",
                reasoning_effort.trim(),
                self,
                &mut on_event,
            )?;
            session.model_options.current_reasoning_effort =
                Some(reasoning_effort.trim().to_string());
        }

        Ok(session.model_options.clone())
    }

    // This public boundary mirrors the explicit ACP/Tauri request fields.
    #[allow(clippy::too_many_arguments)]
    pub fn prompt<F>(
        &self,
        text: impl AsRef<str>,
        cwd: Option<String>,
        profile_id: Option<String>,
        context: Option<VideoPromptContext>,
        history_context: Option<String>,
        saved_session: Option<SavedSessionHint>,
        client_settings: AcpClientSettings,
        profiles: AgentProfilesHint,
        on_event: F,
    ) -> Result<String, AcpError>
    where
        F: FnMut(AcpEvent),
    {
        self.prompt_with_label(
            text,
            cwd,
            profile_id,
            context,
            history_context,
            saved_session,
            client_settings,
            profiles,
            on_event,
            None,
        )
    }

    /// Same flow with an optional attempt label for log correlation. The label
    /// is metadata only (job/batch/attempts); it is logged, never parsed and
    /// never sent to the model. `None` preserves the exact chat behavior.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prompt_with_label<F>(
        &self,
        text: impl AsRef<str>,
        cwd: Option<String>,
        profile_id: Option<String>,
        context: Option<VideoPromptContext>,
        history_context: Option<String>,
        saved_session: Option<SavedSessionHint>,
        client_settings: AcpClientSettings,
        profiles: AgentProfilesHint,
        mut on_event: F,
        attempt_label: Option<&str>,
    ) -> Result<String, AcpError>
    where
        F: FnMut(AcpEvent),
    {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            // Best-effort pid peek: never block the failing path on the lock.
            let pid = self
                .session
                .try_lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|session| session.child.id()));
            Self::log_workshop_exit(attempt_label, pid, "busy", None, 0);
            return Err(AcpError::busy());
        }
        self.cancel.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.permission_mode.lock() {
            *guard = client_settings.permission_mode;
        }

        let prepared = prepare_profiles(&profiles);
        let outcome = self.run_prompt_inner(
            text.as_ref(),
            cwd.as_deref(),
            profile_id.as_deref(),
            context.as_ref(),
            history_context.as_deref(),
            saved_session.as_ref(),
            &prepared,
            &mut on_event,
            attempt_label,
        );

        if outcome.is_err() {
            self.drop_live_session(true);
        }

        self.busy.store(false, Ordering::SeqCst);

        match &outcome {
            Ok((text, stop_reason)) => on_event(AcpEvent::Finished {
                text: text.clone(),
                stop_reason: stop_reason.clone(),
            }),
            Err(error) if error.code == crate::AcpErrorCode::Cancelled => {
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

    /// New home: `crate::jobs::isolated::prompt_isolated_restricted`.
    /// Kept here as a thin forwarder so `AcpService::prompt_isolated_restricted`
    /// paths do not break.
    pub fn prompt_isolated_restricted(
        text: impl AsRef<str>,
        profile_id: String,
        profiles: AgentProfilesHint,
        model_selection: Option<AcpSessionModelSelection>,
        task_label: Option<String>,
    ) -> Result<String, AcpError> {
        crate::jobs::isolated::prompt_isolated_restricted(
            text,
            profile_id,
            profiles,
            model_selection,
            task_label,
        )
    }

    /// New home: `crate::jobs::isolated::discover_isolated_models`.
    /// Kept here as a thin forwarder.
    pub fn discover_isolated_models(
        profile_id: String,
        profiles: AgentProfilesHint,
    ) -> Result<AcpModelDiscoveryResult, AcpError> {
        crate::jobs::isolated::discover_isolated_models(profile_id, profiles)
    }

    // Kept explicit so session lifecycle fields remain visible at the protocol boundary.
    #[allow(clippy::too_many_arguments)]
    fn run_prompt_inner(
        &self,
        prompt_text: &str,
        cwd: Option<&str>,
        profile_id: Option<&str>,
        context: Option<&VideoPromptContext>,
        history_context: Option<&str>,
        saved_session: Option<&SavedSessionHint>,
        prepared: &PreparedProfiles,
        on_event: &mut dyn FnMut(AcpEvent),
        attempt_label: Option<&str>,
    ) -> Result<(String, Option<String>), AcpError> {
        let prompt_text = prompt_text.trim();
        if prompt_text.is_empty() {
            Self::log_workshop_exit(attempt_label, None, "empty-prompt", None, 0);
            return Err(AcpError::bad_request("提问内容不能为空"));
        }

        let profile_override = profile_id;

        // Ensure live session (reuse when possible).
        {
            let mut guard = match self.session.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    Self::log_workshop_exit(attempt_label, None, "session-lock-failed", None, 0);
                    return Err(AcpError::internal(Some("ACP session mutex poisoned")));
                }
            };
            if guard.is_none() {
                // Snapshot IO goes through the app-provided environment (M5);
                // missing snapshot still means vision-capable, as before.
                let vision_capable = resolve_session_cwd(cwd)
                    .ok()
                    .and_then(|workspace| {
                        session_env()
                            .ok()
                            .and_then(|env| env.snapshot_vision_capable(&workspace))
                    })
                    .unwrap_or(true);
                let mut spawned = match self.spawn_session(
                    cwd,
                    saved_session,
                    prepared,
                    profile_override,
                    vision_capable,
                    on_event,
                ) {
                    Ok(spawned) => spawned,
                    Err(error) => {
                        Self::log_workshop_exit(attempt_label, None, "spawn-failed", None, 0);
                        return Err(error);
                    }
                };
                let selection = self
                    .next_session_model_selection
                    .lock()
                    .ok()
                    .and_then(|mut selection| selection.take());
                if let Some(selection) = selection {
                    if let Err(error) =
                        self.apply_model_selection(&mut spawned, &selection, on_event)
                    {
                        Self::log_workshop_exit(
                            attempt_label,
                            Some(spawned.child.id()),
                            "model-selection-failed",
                            None,
                            0,
                        );
                        return Err(error);
                    }
                }
                *guard = Some(spawned);
            }
        }

        let mut guard = match self.session.lock() {
            Ok(guard) => guard,
            Err(_) => {
                Self::log_workshop_exit(attempt_label, None, "session-lock-failed", None, 0);
                return Err(AcpError::internal(Some("ACP session mutex poisoned")));
            }
        };
        let session = match guard.as_mut() {
            Some(session) => session,
            None => {
                Self::log_workshop_exit(attempt_label, None, "session-missing", None, 0);
                return Err(AcpError::internal(Some("ACP session missing after spawn")));
            }
        };
        // Copy: the EOF path takes the guard, so the kind must be owned here.
        let profile_kind = session.profile_kind;

        if let Err(error) = resolve_session_cwd(cwd) {
            Self::log_workshop_exit(
                attempt_label,
                Some(session.child.id()),
                "cwd-failed",
                None,
                0,
            );
            return Err(error);
        }

        on_event(AcpEvent::Progress {
            message: "正在发送问题…".into(),
        });

        let prompt_id = session.next_id;
        session.next_id += 1;
        let pid = session.child.id();
        if let Err(error) = crate::runtime::io::write_request(
            &mut session.stdin,
            prompt_id,
            "session/prompt",
            crate::domain::context::session_prompt_params(
                &session.session_id,
                prompt_text,
                context,
                history_context,
            ),
        ) {
            Self::log_workshop_exit(attempt_label, Some(pid), "prompt-write-failed", None, 0);
            return Err(error);
        }
        // Attempt start marker: present only for labeled (workshop) calls so
        // chat traffic is untouched. Every attempt is traceable even if the
        // outcome arms below never run (timeout/cancel take other exits).
        if let Some(label) = attempt_label {
            tracing::info!(task_label = %label, pid, "workshop prompt sent");
        }

        let mut collector = AgentReplyCollector::default();
        let mut on_event_collect = |ev: AcpEvent| {
            match &ev {
                AcpEvent::AgentMessage { text } => collector.push_agent_chunk(text),
                AcpEvent::ToolCall { .. } => collector.on_tool_call(),
                _ => {}
            }
            on_event(ev);
        };

        let deadline =
            Instant::now() + Duration::from_secs(crate::runtime::io::PROMPT_DEADLINE_SECS);
        let mut cancel_sent = false;
        let mut cancel_at: Option<Instant> = None;

        loop {
            if self.cancel.load(Ordering::SeqCst) && !cancel_sent {
                let _ = crate::runtime::io::write_notification(
                    &mut session.stdin,
                    "session/cancel",
                    session_cancel_params(&session.session_id),
                );
                cancel_sent = true;
                cancel_at = Some(Instant::now());
            }

            if Instant::now() > deadline {
                if let Some(mut taken) = guard.take() {
                    crate::runtime::process::terminate_tree(&mut taken.child, false);
                }
                Self::log_workshop_exit(
                    attempt_label,
                    Some(pid),
                    "timeout",
                    None,
                    collector.chunk_count(),
                );
                return Err(AcpError::protocol(Some("ACP wait timed out")));
            }

            if let Some(at) = cancel_at {
                if Instant::now().duration_since(at)
                    > Duration::from_secs(crate::runtime::io::CANCEL_KILL_SECS)
                {
                    if let Some(mut taken) = guard.take() {
                        crate::runtime::process::terminate_tree(&mut taken.child, false);
                    }
                    Self::log_workshop_exit(
                        attempt_label,
                        Some(pid),
                        "cancel-timeout",
                        None,
                        collector.chunk_count(),
                    );
                    return Err(AcpError::cancelled());
                }
            }

            let inbound = match crate::runtime::io::read_one(
                self,
                session,
                Duration::from_millis(250),
                &self.cancel,
                &self.host,
                &mut on_event_collect,
            ) {
                Ok(inbound) => inbound,
                Err(error) => {
                    Self::log_workshop_exit(
                        attempt_label,
                        Some(pid),
                        "read-error",
                        None,
                        collector.chunk_count(),
                    );
                    return Err(error);
                }
            };
            match inbound {
                crate::runtime::io::ReadOne::Eof => {
                    let _ = guard.take();
                    if self.cancel.load(Ordering::SeqCst) {
                        Self::log_workshop_exit(
                            attempt_label,
                            Some(pid),
                            "cancelled",
                            None,
                            collector.chunk_count(),
                        );
                        return Err(AcpError::cancelled());
                    }
                    break;
                }
                crate::runtime::io::ReadOne::Response { id, value } if id == prompt_id => {
                    if let Some(msg) = is_error_response(&value) {
                        tracing::warn!(%msg, "ACP prompt error response");
                        Self::log_workshop_exit(
                            attempt_label,
                            Some(pid),
                            "agent-error",
                            None,
                            collector.chunk_count(),
                        );
                        return Err(AcpError::protocol(Some(&msg)));
                    }
                    let stop = parse_stop_reason(&value);
                    if stop.as_deref() == Some("cancelled") || self.cancel.load(Ordering::SeqCst) {
                        Self::log_workshop_exit(
                            attempt_label,
                            Some(pid),
                            "cancelled",
                            stop.as_deref(),
                            collector.chunk_count(),
                        );
                        return Err(AcpError::cancelled());
                    }
                    let chunks = collector.chunk_count();
                    let final_text = collector.finish();
                    if final_text.trim().is_empty() {
                        Self::log_workshop_exit(
                            attempt_label,
                            Some(pid),
                            "no-output",
                            stop.as_deref(),
                            chunks,
                        );
                        return self.empty_reply_outcome(profile_kind, stop);
                    }
                    Self::log_workshop_exit(
                        attempt_label,
                        Some(pid),
                        "ok",
                        stop.as_deref(),
                        chunks,
                    );
                    return Ok((final_text, stop));
                }
                crate::runtime::io::ReadOne::Response { .. } => continue,
            }
        }

        if self.cancel.load(Ordering::SeqCst) {
            let _ = guard.take();
            Self::log_workshop_exit(
                attempt_label,
                Some(pid),
                "cancelled",
                None,
                collector.chunk_count(),
            );
            return Err(AcpError::cancelled());
        }
        let chunks = collector.chunk_count();
        let final_text = collector.finish();
        if final_text.trim().is_empty() {
            Self::log_workshop_exit(
                attempt_label,
                Some(pid),
                "no-output",
                Some("end_turn"),
                chunks,
            );
            return self.empty_reply_outcome(profile_kind, Some("end_turn".into()));
        }
        Self::log_workshop_exit(attempt_label, Some(pid), "ok", Some("end_turn"), chunks);
        Ok((final_text, Some("end_turn".into())))
    }

    /// Isolated AI tasks (translation/polishing) run tool-free: no MCP tools,
    /// no Chat snapshot/history reuse.
    pub(crate) fn isolated_task(&self) -> bool {
        !self.tool_access_enabled.load(Ordering::SeqCst)
    }

    /// P0b attempt telemetry: every workshop attempt exit logs the same
    /// fields (label/pid/outcome/stop/chunks). Chat (label None) is untouched.
    /// Pid-missing (pre-spawn) is logged explicitly, never silent. `outcome`
    /// is a fixed tag per exit site; failure reasons travel in the returned
    /// error as before.
    fn log_workshop_exit(
        attempt_label: Option<&str>,
        pid: Option<u32>,
        outcome: &str,
        stop: Option<&str>,
        chunks: u32,
    ) {
        let Some(label) = attempt_label else {
            return;
        };
        match pid {
            Some(pid) => tracing::info!(
                task_label = %label,
                pid,
                outcome,
                stop = ?stop,
                chunks,
                "workshop prompt exit"
            ),
            None => tracing::info!(
                task_label = %label,
                outcome,
                stop = ?stop,
                chunks,
                "workshop prompt exit (no child yet)"
            ),
        }
    }

    /// Zero-output outcome split by caller kind. Chat keeps the human-readable
    /// hint as a successful reply (existing UX); isolated tasks get a typed
    /// `NoOutput` error so callers retry or fail loudly instead of parsing
    /// hint prose as JSON. Unit-covered without a live agent process.
    fn empty_reply_outcome(
        &self,
        profile_kind: AgentKind,
        stop: Option<String>,
    ) -> Result<(String, Option<String>), AcpError> {
        if self.isolated_task() {
            return Err(AcpError::no_output(stop.as_deref()));
        }
        let empty_hint = match profile_kind {
            AgentKind::Codex => {
                "（会话结束，未解析到文本回复；请确认 Codex 已登录，且模型走 Responses API）"
            }
            _ => "（会话结束，未解析到文本回复；请确认该 ACP Agent 可用）",
        };
        Ok((empty_hint.into(), stop))
    }

    pub(crate) fn apply_model_selection(
        &self,
        session: &mut crate::runtime::lifecycle::LiveSession,
        selection: &AcpSessionModelSelection,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(), AcpError> {
        if selection.model_id.trim().is_empty() {
            return Ok(());
        }
        Self::set_session_config_option(session, "model", &selection.model_id, self, on_event)?;
        if let Some(reasoning_effort) = selection
            .reasoning_effort
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Self::set_session_config_option(
                session,
                "reasoning_effort",
                reasoning_effort,
                self,
                on_event,
            )?;
        }
        Ok(())
    }

    fn set_session_config_option(
        session: &mut crate::runtime::lifecycle::LiveSession,
        config_id: &str,
        value: &str,
        service: &AcpService,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(), AcpError> {
        let request_id = session.next_id;
        session.next_id += 1;
        crate::runtime::io::write_request(
            &mut session.stdin,
            request_id,
            "session/set_config_option",
            crate::wire::session::session_set_config_option_params(
                &session.session_id,
                config_id,
                value,
            ),
        )?;
        let response = crate::runtime::io::read_until_id_raw(
            service,
            session,
            request_id,
            Duration::from_secs(30),
            &service.cancel,
            &service.host,
            on_event,
        )?;
        if let Some(message) = is_error_response(&response) {
            return Err(AcpError::protocol(Some(&format!(
                "session config {config_id}: {message}"
            ))));
        }
        Ok(())
    }
}

impl Default for AcpService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    #[test]
    fn prompt_loop_bounds_are_locked() {
        // These bounds are the product contract for H-P2-7 (timeout + cancel).
        // Change them deliberately, never by accident.
        assert_eq!(crate::runtime::io::PROMPT_DEADLINE_SECS, 600);
        assert_eq!(crate::runtime::io::CANCEL_KILL_SECS, 8);
        assert!(crate::runtime::io::initialize_timeout() >= Duration::from_secs(60));
    }

    /// Fabricate a slot session around a process that already exited: stdin
    /// writes fail fast (broken pipe), so rotation IO fails deterministically
    /// without any agent or network.
    fn dead_slot_session(supports_session_close: bool) -> crate::runtime::lifecycle::LiveSession {
        use std::io::BufReader;
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "exit", "0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn cmd");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        child.wait().expect("reap");
        crate::runtime::lifecycle::LiveSession {
            child,
            stdin,
            reader: BufReader::new(stdout),
            session_id: "test-session".to_string(),
            next_id: 1,
            init: crate::wire::session::InitializeResult {
                supports_session_close,
                ..crate::wire::session::InitializeResult::default()
            },
            profile_kind: AgentKind::Codex,
            model_options: AcpSessionModelOptions::default(),
        }
    }

    fn slot_occupied(service: &AcpService) -> bool {
        service.session.lock().expect("lock").is_some()
    }

    #[test]
    fn rotation_close_failure_leaves_slot_empty() {
        // session/close write hits a dead pipe → Err. The slot must be empty
        // afterwards so the next submit spawns fresh instead of reusing the
        // closed session. Take-upfront structure makes this hold for every
        // failure path, not just this one.
        let service = AcpService::new_isolated(None);
        service
            .session
            .lock()
            .expect("lock")
            .replace(dead_slot_session(true));
        assert!(slot_occupied(&service));
        let _ = service
            .rotate_isolated_session("p", None, &mut |_| {})
            .expect_err("close must fail");
        assert!(
            !slot_occupied(&service),
            "failed rotation must leave the slot empty"
        );
    }

    #[test]
    fn rotation_without_close_primitive_drops_process() {
        let service = AcpService::new_isolated(None);
        service
            .session
            .lock()
            .expect("lock")
            .replace(dead_slot_session(false));
        service
            .rotate_isolated_session("p", None, &mut |_| {})
            .expect("no-op ok");
        assert!(!slot_occupied(&service));
    }

    #[test]
    fn rearm_restores_consumed_model_selection() {
        let selection = AcpSessionModelSelection {
            model_id: "test-model".to_string(),
            reasoning_effort: Some("low".to_string()),
        };
        let service = AcpService::new_isolated(Some(selection.clone()));
        // The fresh-spawn path takes the one-shot cell: simulate the first
        // prompt's consume, then re-arm like every pool run does.
        let taken = service
            .next_session_model_selection
            .lock()
            .expect("lock")
            .take();
        assert_eq!(
            taken.map(|selected| selected.model_id),
            Some("test-model".to_string())
        );
        service.rearm_isolated_model_selection(Some(selection));
        let restored = service
            .next_session_model_selection
            .lock()
            .expect("lock")
            .clone()
            .expect("rearmed");
        assert_eq!(restored.model_id, "test-model");
        assert_eq!(restored.reasoning_effort.as_deref(), Some("low"));
    }

    #[test]
    fn empty_reply_outcome_splits_chat_and_isolated() {
        use std::sync::atomic::Ordering;

        // Chat (tools enabled): keeps the human-readable hint as success.
        let chat = AcpService::new();
        let (text, stop) = chat
            .empty_reply_outcome(crate::agent::profile::AgentKind::Codex, None)
            .expect("chat keeps hint");
        assert!(text.contains("Codex"));
        assert_eq!(stop, None);

        // Isolated task: typed error carrying the stop reason, never prose.
        let isolated = AcpService::new();
        isolated.tool_access_enabled.store(false, Ordering::SeqCst);
        let err = isolated
            .empty_reply_outcome(
                crate::agent::profile::AgentKind::Codex,
                Some("end_turn".into()),
            )
            .expect_err("isolated errors");
        assert_eq!(err.code, crate::AcpErrorCode::NoOutput);
        assert_eq!(err.details.as_deref(), Some("end_turn"));
    }
}
