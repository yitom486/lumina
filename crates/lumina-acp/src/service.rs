//! AcpService — optional on-demand ACP Client over stdio JSON-RPC.
//!
//! Baseline Client→Agent: initialize, authenticate, session/new|prompt|cancel|close.
//! Agent→Client: session/update, session/request_permission, fs/*, terminal/*.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::agent_reply_collector::AgentReplyCollector;
use crate::context::{self, VideoPromptContext};
use crate::environment::session_env;
use crate::error::AcpError;
use crate::host::AcpHost;
use crate::model::{
    AcpEvent, AcpModelDiscoveryResult, AcpSessionModelOptions, AcpSessionModelSelection, AcpStatus,
    AgentProfilesHint, PermissionOption, SavedSessionHint,
};
use crate::paths::{resolve_session_cwd, status_from_profiles};
use crate::profile::{
    prepare_profiles, resolve_active_profile, resolve_launch, AgentKind, PreparedProfiles,
};
use crate::protocol::{
    authenticate_params, classify_inbound, encode_line, error_response, extract_agent_text,
    extract_permission_options, extract_plan_summary, extract_thought_text, extract_tool_call,
    extract_tool_call_content_chunk, initialize_params, initialize_params_restricted,
    is_error_response, notification, parse_initialize_result, parse_session_id,
    parse_session_model_options, parse_stop_reason, permission_auto_result,
    permission_cancelled_result, permission_selected_result, pick_auth_method, request,
    session_cancel_params, session_close_params, session_new_params, session_resume_params,
    session_set_config_option_params, success_response, Inbound, InitializeResult,
};
use crate::settings::{AcpClientSettings, PermissionMode};

/// Prompt-loop bounds, locked by unit test (silent timeout removal must fail loudly).
const PROMPT_DEADLINE_SECS: u64 = 600;
/// Grace period for an agent to honor `session/cancel` before the process is killed.
const CANCEL_KILL_SECS: u64 = 8;

struct LiveSession {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<std::process::ChildStdout>,
    session_id: String,
    next_id: u64,
    init: InitializeResult,
    profile_kind: AgentKind,
    model_options: AcpSessionModelOptions,
}

/// P1: a LiveSession always owns a live agent tree. Any path that drops one
/// without an explicit close (setup failure, abandoned take, early return)
/// terminates the whole tree instead of leaking it. Never blocks (`false`):
/// explicit close paths terminate deterministically first, and a repeated
/// taskkill against the dead tree fails silently.
impl Drop for LiveSession {
    fn drop(&mut self) {
        crate::process::terminate_tree(&mut self.child, false);
    }
}

pub struct AcpService {
    busy: AtomicBool,
    cancel: AtomicBool,
    session: Mutex<Option<LiveSession>>,
    host: AcpHost,
    permission_mode: Mutex<PermissionMode>,
    permission_replies: Mutex<Option<mpsc::Sender<Option<String>>>>,
    permission_seq: AtomicU64,
    tool_access_enabled: AtomicBool,
    next_session_model_selection: Mutex<Option<AcpSessionModelSelection>>,
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

    /// Workshop pool slot: tool access disabled from birth, with a preset
    /// model selection applied on first use by the normal prompt flow.
    pub(crate) fn new_isolated(model_selection: Option<AcpSessionModelSelection>) -> Self {
        let service = Self::new();
        service.tool_access_enabled.store(false, Ordering::SeqCst);
        service.rearm_isolated_model_selection(model_selection);
        service
    }

    /// Restore the slot's model selection into the one-shot cell (P2 pool).
    /// The fresh-spawn path takes the cell on first use, so every submit
    /// must re-arm it BEFORE prompting: otherwise a fresh spawn after a
    /// rotation failure or a transport retry silently falls back to the
    /// agent default model. The slot (not the service) owns the config, so
    /// this never leaks across jobs.
    pub(crate) fn rearm_isolated_model_selection(
        &self,
        model_selection: Option<AcpSessionModelSelection>,
    ) {
        if let Ok(mut selection) = self.next_session_model_selection.lock() {
            *selection = model_selection;
        }
    }

    /// Rotate the live session without killing the process (P2 pool support):
    /// close the old session when the agent supports it, then open a fresh
    /// one on the same child, reapplying the model selection. No live session
    /// is a successful no-op (the next prompt spawns fresh).
    ///
    /// The old session is taken out of the slot FIRST, and only reinserted
    /// after every step succeeds. Any failure therefore leaves the slot
    /// empty by construction, so the next submit spawns fresh instead of
    /// running on a closed or half-open session. No failure path may put a
    /// dead session back.
    pub(crate) fn rotate_isolated_session(
        &self,
        profile_id: &str,
        model_selection: Option<&AcpSessionModelSelection>,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(), AcpError> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        let Some(mut session) = guard.take() else {
            return Ok(());
        };
        if !session.init.supports_session_close {
            // No rotation primitive: the take above already dropped the whole
            // process (see `Drop for LiveSession`); the next prompt spawns
            // fresh instead of stacking sessions server-side.
            return Ok(());
        }
        if let Err(error) = Self::close_agent_session(self, &mut session, on_event) {
            tracing::warn!(%error, "session/close failed during rotation; slot left empty");
            return Err(error);
        }
        let workspace = match resolve_session_cwd(None) {
            Ok(workspace) => workspace,
            Err(error) => {
                tracing::warn!(%error, "workspace unavailable during rotation; slot left empty");
                return Err(error);
            }
        };
        let cwd = workspace.to_string_lossy().into_owned();
        let new_id = match self.create_new_session(&mut session, &cwd, profile_id, false, on_event)
        {
            Ok(id) => id,
            Err(error) => {
                tracing::warn!(%error, "session/new failed during rotation; slot left empty");
                return Err(error);
            }
        };
        session.session_id = new_id;
        if let Some(selection) = model_selection {
            if let Err(error) = self.apply_model_selection(&mut session, selection, on_event) {
                tracing::warn!(%error, "model selection failed during rotation; slot left empty");
                return Err(error);
            }
        }
        *guard = Some(session);
        Ok(())
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
                let _ = Self::write_notification(
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

    fn close_agent_session(
        service: &Self,
        session: &mut LiveSession,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(), AcpError> {
        let request_id = session.next_id;
        session.next_id += 1;
        Self::write_request(
            &mut session.stdin,
            request_id,
            "session/close",
            session_close_params(&session.session_id),
        )?;
        let response = Self::read_until_id_raw(
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
                "session/close: {message}"
            ))));
        }
        Ok(())
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

    fn drop_live_session(&self, wait_for_child: bool) {
        if let Ok(mut guard) = self.session.lock() {
            if let Some(mut session) = guard.take() {
                if wait_for_child && session.init.supports_session_close {
                    let id = session.next_id;
                    session.next_id += 1;
                    let _ = Self::write_request(
                        &mut session.stdin,
                        id,
                        "session/close",
                        session_close_params(&session.session_id),
                    );
                }
                // P1: kill the whole tree (wrapper-only kill orphaned the
                // second Codex process). Graceful close above stays first.
                crate::process::terminate_tree(&mut session.child, wait_for_child);
            }
        }
        if wait_for_child {
            self.host.release_all();
        } else {
            self.host.release_all_for_shutdown();
        }
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

    /// Fixed client settings for isolated workshop prompts (tool-free, blind
    /// to chat state). Shared by the one-shot path and the P2 pool runner so
    /// the two never drift apart.
    pub(crate) fn isolated_client_settings() -> AcpClientSettings {
        AcpClientSettings {
            permission_mode: PermissionMode::Ask,
            thinking_level: crate::settings::ThinkingLevel::Hidden,
            agent_mode: "subtitle-workshop".into(),
            vision_capable: false,
            model_id: None,
            reasoning_effort: None,
        }
    }

    /// Run one prompt in a fresh ACP process/session using an existing profile,
    /// then close it. This is deliberately separate from the interactive chat
    /// service: no saved session, no chat context, and no Agent tool access.
    pub fn prompt_isolated_restricted(
        text: impl AsRef<str>,
        profile_id: String,
        profiles: AgentProfilesHint,
        model_selection: Option<AcpSessionModelSelection>,
        task_label: Option<String>,
    ) -> Result<String, AcpError> {
        let service = Self::new();
        service.tool_access_enabled.store(false, Ordering::SeqCst);
        if let Ok(mut selection) = service.next_session_model_selection.lock() {
            *selection = model_selection;
        }
        let outcome = service.prompt_with_label(
            text,
            None,
            Some(profile_id),
            None,
            None,
            None,
            Self::isolated_client_settings(),
            profiles,
            |_| {},
            task_label.as_deref(),
        );
        let _ = service.close_session();
        outcome
    }

    /// Explicitly opens a short-lived, tool-disabled session and returns only
    /// its model configuration options. No user/media data is sent.
    pub fn discover_isolated_models(
        profile_id: String,
        profiles: AgentProfilesHint,
    ) -> Result<AcpModelDiscoveryResult, AcpError> {
        let service = Self::new();
        service.tool_access_enabled.store(false, Ordering::SeqCst);
        let prepared = prepare_profiles(&profiles);
        let session =
            service.spawn_session(None, None, &prepared, Some(&profile_id), true, &mut |_| {})?;
        let options = session.model_options.clone();
        if let Ok(mut guard) = service.session.lock() {
            *guard = Some(session);
        }
        let _ = service.close_session();
        Ok(AcpModelDiscoveryResult {
            connected: true,
            message: if options.models.is_empty() {
                "Agent 已连接，但未提供可选择的模型；将使用 Agent 默认模型".into()
            } else {
                "Agent 已连接，请选择用于媒体匹配的低成本模型与推理强度".into()
            },
            options,
        })
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
        if let Err(error) = Self::write_request(
            &mut session.stdin,
            prompt_id,
            "session/prompt",
            context::session_prompt_params(
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

        let deadline = Instant::now() + Duration::from_secs(PROMPT_DEADLINE_SECS);
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
                if let Some(mut taken) = guard.take() {
                    crate::process::terminate_tree(&mut taken.child, false);
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
                if Instant::now().duration_since(at) > Duration::from_secs(CANCEL_KILL_SECS) {
                    if let Some(mut taken) = guard.take() {
                        crate::process::terminate_tree(&mut taken.child, false);
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

            let inbound = match Self::read_one(
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
                ReadOne::Eof => {
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
                ReadOne::Response { id, value } if id == prompt_id => {
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
                ReadOne::Response { .. } => continue,
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
    fn isolated_task(&self) -> bool {
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

    fn spawn_session(
        &self,
        cwd_hint: Option<&str>,
        saved_session: Option<&SavedSessionHint>,
        prepared: &PreparedProfiles,
        profile_id: Option<&str>,
        vision_capable: bool,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<LiveSession, AcpError> {
        let workspace = resolve_session_cwd(cwd_hint)?;
        let cwd = workspace.to_string_lossy().to_string();
        self.host.set_workspace(workspace.clone());
        // Clear workspace if session setup fails after this point.
        struct ClearWorkspaceOnDrop<'a>(&'a AcpHost, bool);
        impl Drop for ClearWorkspaceOnDrop<'_> {
            fn drop(&mut self) {
                if self.1 {
                    self.0.clear_workspace();
                }
            }
        }
        let mut workspace_guard = ClearWorkspaceOnDrop(&self.host, true);

        let profile = resolve_active_profile(prepared, profile_id)?;
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
        on_event(AcpEvent::Progress {
            message: format!("工作目录：{cwd}"),
        });

        let mut command = crate::process::command(&launch.program);
        command
            .args(&launch.args)
            .current_dir(&workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &launch.env {
            command.env(key, value);
        }

        tracing::info!(
            profile_id = %profile.id,
            profile_kind = ?profile.kind,
            program = %launch.program.display(),
            argument_count = launch.args.len(),
            cwd = %workspace.display(),
            "ACP launch prepared"
        );
        let mut child = command.spawn().map_err(|error| {
            tracing::warn!(
                stage = "spawn",
                profile_id = %profile.id,
                program = %launch.program.display(),
                cwd = %workspace.display(),
                error_kind = ?error.kind(),
                os_error = ?error.raw_os_error(),
                %error,
                "ACP spawn failed"
            );
            AcpError::spawn_failed(Some(&format!(
                "stage=spawn; program={}; cwd={}; kind={:?}; os_error={:?}; error={error}",
                launch.program.display(),
                workspace.display(),
                error.kind(),
                error.raw_os_error()
            )))
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

        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                crate::process::terminate_tree(&mut child, false);
                return Err(AcpError::spawn_failed(Some("stdin pipe missing")));
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                crate::process::terminate_tree(&mut child, false);
                return Err(AcpError::spawn_failed(Some("stdout pipe missing")));
            }
        };
        let reader = BufReader::new(stdout);

        let mut session = LiveSession {
            child,
            stdin,
            reader,
            session_id: String::new(),
            next_id: 1,
            init: InitializeResult::default(),
            profile_kind: profile.kind,
            model_options: AcpSessionModelOptions::default(),
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
            if self.tool_access_enabled.load(Ordering::SeqCst) {
                initialize_params()
            } else {
                initialize_params_restricted()
            },
        )?;
        let init_resp = Self::read_until_id_raw(
            self,
            &mut session,
            init_id,
            initialize_timeout(),
            &self.cancel,
            &self.host,
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

        // Authenticate when Agent advertises methods — prefer ChatGPT when ~/.codex exists.
        if let Some(method) = pick_auth_method(&init) {
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
                self,
                &mut session,
                auth_id,
                Duration::from_secs(120),
                &self.cancel,
                &self.host,
                on_event,
            )?;
            if let Some(msg) = is_error_response(&auth_resp) {
                crate::process::terminate_tree(&mut session.child, false);
                if profile.kind == AgentKind::Codex {
                    return Err(AcpError::codex_auth_required(Some(&format!(
                        "authenticate failed: {msg}"
                    ))));
                }
                return Err(AcpError::protocol(Some(&format!(
                    "authenticate failed: {msg}"
                ))));
            }
        }

        session.init = init.clone();
        let supports_resume = init.supports_session_resume;

        // Snapshot IO goes through the app-provided environment (M5).
        let env = session_env()?;
        let snapshot_path = env.snapshot_path(std::path::Path::new(&cwd));
        env.sync_snapshot(&snapshot_path, vision_capable)
            .map_err(|error| AcpError::internal(Some(&error)))?;
        let mcp_servers = env.mcp_servers(&snapshot_path, self.isolated_task());

        on_event(AcpEvent::Progress {
            message: "正在创建会话…".into(),
        });

        let saved = saved_session.cloned();
        let try_resume = supports_resume
            && saved.as_ref().is_some_and(|s| {
                s.profile_id == profile.id && s.cwd == cwd && !s.session_id.is_empty()
            });

        let session_id = if try_resume {
            let Some(saved) = saved else {
                return Err(AcpError::internal(Some("saved session missing for resume")));
            };
            on_event(AcpEvent::Progress {
                message: "正在恢复上次会话…".into(),
            });
            let resume_id = session.next_id;
            session.next_id += 1;
            Self::write_request(
                &mut session.stdin,
                resume_id,
                "session/resume",
                session_resume_params(&saved.session_id, &cwd, mcp_servers),
            )?;
            let resume_resp = Self::read_until_id_raw(
                self,
                &mut session,
                resume_id,
                Duration::from_secs(60),
                &self.cancel,
                &self.host,
                on_event,
            );
            match resume_resp {
                Ok(response) => {
                    session.model_options = parse_session_model_options(&response);
                    on_event(AcpEvent::Progress {
                        message: "已恢复上次会话".into(),
                    });
                    on_event(AcpEvent::SessionSaved {
                        session_id: saved.session_id.clone(),
                        profile_id: profile.id.clone(),
                        cwd: cwd.clone(),
                    });
                    saved.session_id
                }
                Err(error) => {
                    tracing::warn!(%error, "session/resume failed; creating new session");
                    self.create_new_session(
                        &mut session,
                        &cwd,
                        &profile.id,
                        vision_capable,
                        on_event,
                    )?
                }
            }
        } else {
            self.create_new_session(&mut session, &cwd, &profile.id, vision_capable, on_event)?
        };

        session.session_id = session_id;
        workspace_guard.1 = false;
        Ok(session)
    }

    fn create_new_session(
        &self,
        session: &mut LiveSession,
        cwd: &str,
        profile_id: &str,
        vision_capable: bool,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<String, AcpError> {
        // Snapshot IO goes through the app-provided environment (M5).
        let env = session_env()?;
        let snapshot_path = env.snapshot_path(std::path::Path::new(cwd));
        env.sync_snapshot(&snapshot_path, vision_capable)
            .map_err(|error| AcpError::internal(Some(&error)))?;
        let new_id = session.next_id;
        session.next_id += 1;
        let mcp_servers = env.mcp_servers(&snapshot_path, self.isolated_task());
        tracing::info!(
            cwd,
            snapshot = %snapshot_path.display(),
            mcp = %mcp_servers,
            "registering Lumina MCP for ACP session"
        );
        Self::write_request(
            &mut session.stdin,
            new_id,
            "session/new",
            session_new_params(cwd, mcp_servers),
        )?;
        let session_resp = Self::read_until_id_raw(
            self,
            session,
            new_id,
            Duration::from_secs(60),
            &self.cancel,
            &self.host,
            on_event,
        )?;
        let Some(session_id) = parse_session_id(&session_resp) else {
            crate::process::terminate_tree(&mut session.child, false);
            return Err(AcpError::protocol(Some(&format!(
                "missing sessionId: {session_resp}"
            ))));
        };
        session.model_options = parse_session_model_options(&session_resp);
        on_event(AcpEvent::SessionSaved {
            session_id: session_id.clone(),
            profile_id: profile_id.to_string(),
            cwd: cwd.to_string(),
        });
        Ok(session_id)
    }

    fn apply_model_selection(
        &self,
        session: &mut LiveSession,
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
        session: &mut LiveSession,
        config_id: &str,
        value: &str,
        service: &AcpService,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(), AcpError> {
        let request_id = session.next_id;
        session.next_id += 1;
        Self::write_request(
            &mut session.stdin,
            request_id,
            "session/set_config_option",
            session_set_config_option_params(&session.session_id, config_id, value),
        )?;
        let response = Self::read_until_id_raw(
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

    fn handle_permission_request(
        &self,
        id: Value,
        params: &Value,
        canceling: bool,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Value {
        let tool_id = params
            .pointer("/toolCall/toolCallId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let title = params
            .pointer("/toolCall/title")
            .and_then(Value::as_str)
            .map(str::to_string);

        if canceling {
            on_event(AcpEvent::PermissionResolved {
                tool_call_id: tool_id.clone(),
                decision: "cancelled".into(),
            });
            return success_response(id, permission_cancelled_result());
        }

        let permission_mode = self
            .permission_mode
            .lock()
            .map(|guard| *guard)
            .unwrap_or(PermissionMode::Auto);
        if permission_mode == PermissionMode::Auto {
            on_event(AcpEvent::PermissionResolved {
                tool_call_id: tool_id,
                decision: "auto".into(),
            });
            return success_response(id, permission_auto_result(params, false));
        }

        let request_id = format!(
            "perm-{}",
            self.permission_seq.fetch_add(1, Ordering::SeqCst)
        );
        let options: Vec<PermissionOption> = extract_permission_options(params)
            .into_iter()
            .map(|(option_id, name, kind)| PermissionOption {
                option_id,
                name,
                kind,
            })
            .collect();

        on_event(AcpEvent::PermissionRequest {
            request_id: request_id.clone(),
            tool_call_id: tool_id.clone(),
            title,
            options: options.clone(),
        });

        let (tx, rx) = mpsc::channel();
        if let Ok(mut guard) = self.permission_replies.lock() {
            *guard = Some(tx);
        }

        let selected = rx.recv_timeout(Duration::from_secs(120)).unwrap_or(None);
        let _ = self.permission_replies.lock().map(|mut g| {
            g.take();
        });

        let result = match selected {
            Some(option_id) if !option_id.is_empty() => {
                on_event(AcpEvent::PermissionResolved {
                    tool_call_id: tool_id,
                    decision: "approved".into(),
                });
                permission_selected_result(&option_id)
            }
            _ => {
                on_event(AcpEvent::PermissionResolved {
                    tool_call_id: tool_id,
                    decision: "denied".into(),
                });
                permission_cancelled_result()
            }
        };
        success_response(id, result)
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
        stdin
            .flush()
            .map_err(|error| AcpError::protocol(Some(&format!("flush ACP stdin: {error}"))))?;
        Ok(())
    }

    fn write_raw(stdin: &mut ChildStdin, value: &Value) -> Result<(), AcpError> {
        let line = encode_line(value)?;
        writeln!(stdin, "{line}")
            .map_err(|error| AcpError::protocol(Some(&format!("write ACP response: {error}"))))?;
        stdin
            .flush()
            .map_err(|error| AcpError::protocol(Some(&format!("flush ACP stdin: {error}"))))?;
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
                    detail: tool.detail,
                });
            } else {
                on_event(AcpEvent::ToolCallUpdate {
                    tool_call_id: tool.tool_call_id,
                    status: tool.status,
                    title: tool.title,
                    detail: tool.detail,
                    append_detail: tool.append_detail,
                });
            }
        } else if let Some((tool_call_id, detail)) = extract_tool_call_content_chunk(value) {
            if !detail.is_empty() {
                on_event(AcpEvent::ToolCallUpdate {
                    tool_call_id,
                    status: None,
                    title: None,
                    detail: Some(detail),
                    append_detail: true,
                });
            }
        }
        if let Some(text) = extract_plan_summary(value) {
            on_event(AcpEvent::Plan { text });
        }
    }

    fn handle_inbound_side_effects(
        &self,
        session: &mut LiveSession,
        host: &AcpHost,
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
                if !self.tool_access_enabled.load(Ordering::SeqCst) {
                    let response = error_response(id, -32_001, "Tool access is disabled");
                    Self::write_raw(&mut session.stdin, &response)?;
                    return Ok(None);
                }
                let canceling = cancel.load(Ordering::SeqCst);
                let response = if method == "session/request_permission" {
                    self.handle_permission_request(id, &params, canceling, on_event)
                } else {
                    if method.starts_with("fs/") {
                        on_event(AcpEvent::Progress {
                            message: format!("文件系统：{method}"),
                        });
                    } else if method.starts_with("terminal/") {
                        on_event(AcpEvent::Progress {
                            message: format!("终端：{method}"),
                        });
                    }
                    host.handle_request(&method, id, &params, canceling)
                };
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
        &self,
        session: &mut LiveSession,
        target_id: u64,
        timeout: Duration,
        cancel: &AtomicBool,
        host: &AcpHost,
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
            match Self::read_one(
                self,
                session,
                Duration::from_millis(200),
                cancel,
                host,
                on_event,
            )? {
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
        &self,
        session: &mut LiveSession,
        _wait: Duration,
        cancel: &AtomicBool,
        host: &AcpHost,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<ReadOne, AcpError> {
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            let bytes = session.reader.read_line(&mut line_buf).map_err(|error| {
                tracing::warn!(%error, "ACP read failed");
                AcpError::protocol(Some(&format!("read ACP stdout: {error}")))
            })?;
            if bytes == 0 {
                return Ok(ReadOne::Eof);
            }
            let trimmed = line_buf.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(trimmed) {
                Ok(value) => value,
                Err(error) => {
                    // Truncated sample only: stdout may carry model text.
                    let sample: String = trimmed.chars().take(200).collect();
                    tracing::debug!(line = %sample, %error, "ACP skipping non-JSON stdout line");
                    continue;
                }
            };

            match self.handle_inbound_side_effects(
                session,
                host,
                classify_inbound(value),
                cancel,
                on_event,
            )? {
                Some((id, value)) => return Ok(ReadOne::Response { id, value }),
                None => continue,
            }
        }
    }
}

enum ReadOne {
    Eof,
    Response { id: u64, value: Value },
}

fn initialize_timeout() -> Duration {
    if cfg!(windows) {
        Duration::from_secs(180)
    } else {
        Duration::from_secs(60)
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

    #[test]
    fn prompt_loop_bounds_are_locked() {
        // These bounds are the product contract for H-P2-7 (timeout + cancel).
        // Change them deliberately, never by accident.
        assert_eq!(PROMPT_DEADLINE_SECS, 600);
        assert_eq!(CANCEL_KILL_SECS, 8);
        assert!(initialize_timeout() >= Duration::from_secs(60));
    }

    /// Fabricate a slot session around a process that already exited: stdin
    /// writes fail fast (broken pipe), so rotation IO fails deterministically
    /// without any agent or network.
    fn dead_slot_session(supports_session_close: bool) -> LiveSession {
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
        LiveSession {
            child,
            stdin,
            reader: BufReader::new(stdout),
            session_id: "test-session".to_string(),
            next_id: 1,
            init: InitializeResult {
                supports_session_close,
                ..InitializeResult::default()
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
            .empty_reply_outcome(crate::profile::AgentKind::Codex, None)
            .expect("chat keeps hint");
        assert!(text.contains("Codex"));
        assert_eq!(stop, None);

        // Isolated task: typed error carrying the stop reason, never prose.
        let isolated = AcpService::new();
        isolated.tool_access_enabled.store(false, Ordering::SeqCst);
        let err = isolated
            .empty_reply_outcome(crate::profile::AgentKind::Codex, Some("end_turn".into()))
            .expect_err("isolated errors");
        assert_eq!(err.code, crate::AcpErrorCode::NoOutput);
        assert_eq!(err.details.as_deref(), Some("end_turn"));
    }
}
