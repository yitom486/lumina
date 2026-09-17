//! AcpService — optional on-demand ACP Client over stdio JSON-RPC.
//!
//! Baseline Client→Agent: initialize, authenticate, session/new|prompt|cancel|close.
//! Agent→Client: session/update, session/request_permission, fs/*, terminal/*.
//!
//! Facade only (no behavior change). Lifecycle lives in `super::lifecycle`,
//! line IO in `super::io`, inbound handling in `super::inbound`, isolated
//! workshop tasks in `crate::jobs::isolated`.

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::profile::{prepare_profiles, resolve_active_profile};
use crate::agent::status::status_from_profiles;
use crate::agent::workspace::resolve_session_cwd;
use crate::domain::environment::session_env;
use crate::domain::model::{
    AcpEvent, AcpSessionModelOptions, AcpSessionModelSelection, AcpStatus, AgentProfilesHint,
    SavedSessionHint, SessionKind,
};
use crate::domain::settings::{AcpClientSettings, PermissionMode};
use crate::error::AcpError;
use crate::runtime::host::AcpHost;
use crate::runtime::inbound::handle_inbound_side_effects;
use crate::runtime::io::write_request;
use crate::wire::codec::{classify_inbound, is_error_response};
use crate::wire::session::{session_cancel_params, session_load_params};
use crate::wire::updates::{
    extract_plan_summary, extract_tool_call, extract_tool_call_content_chunk,
};

pub struct AcpService {
    pub(crate) busy: AtomicBool,
    pub(crate) cancel: AtomicBool,
    pub(crate) session: Mutex<Option<crate::runtime::lifecycle::LiveSession>>,
    /// Mirrored cancel target so `session/cancel` never waits on the session
    /// mutex held by the prompt loop across blocking reads. Published when a
    /// live session is installed, cleared when it is taken/dropped. Best
    /// effort: a stale entry only fails a best-effort write; a missing entry
    /// falls back to the session lock.
    pub(crate) cancel_writer: Mutex<Option<(crate::runtime::lifecycle::SharedStdin, String)>>,
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
            cancel_writer: Mutex::new(None),
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

    pub(crate) fn publish_cancel_writer(&self, session: &crate::runtime::lifecycle::LiveSession) {
        if let Ok(mut hook) = self.cancel_writer.lock() {
            *hook = Some((session.stdin.clone(), session.session_id.clone()));
        }
    }

    pub(crate) fn clear_cancel_writer(&self) {
        if let Ok(mut hook) = self.cancel_writer.lock() {
            *hook = None;
        }
    }

    /// Soft-cancel: set flag + send `session/cancel` with priority.
    /// Fast path uses the mirrored writer without touching the session mutex,
    /// so cancel is delivered even while the prompt loop holds that mutex
    /// across a blocking `read_line`. Kill is last-resort after timeout (see
    /// `CANCEL_KILL_SECS` in the prompt loop).
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Ok(hook) = self.cancel_writer.lock() {
            if let Some((stdin, session_id)) = hook.as_ref() {
                let stdin = stdin.clone();
                let session_id = session_id.clone();
                drop(hook);
                let _ = crate::runtime::io::write_notification(
                    &stdin,
                    "session/cancel",
                    session_cancel_params(&session_id),
                );
                return;
            }
        }
        // Fallback for sessions installed before the mirror existed.
        let (stdin, session_id) = match self.session.lock() {
            Ok(guard) => match guard.as_ref() {
                Some(session) => (session.stdin.clone(), session.session_id.clone()),
                None => return,
            },
            Err(poisoned) => match poisoned.into_inner().as_ref() {
                Some(session) => (session.stdin.clone(), session.session_id.clone()),
                None => return,
            },
        };
        let _ = crate::runtime::io::write_notification(
            &stdin,
            "session/cancel",
            session_cancel_params(&session_id),
        );
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
            SessionKind::Chat,
            &mut on_event,
        ) {
            Ok(mut session) => {
                if let Some(selection) = client_settings.model_selection() {
                    self.apply_model_selection(&mut session, &selection, &mut on_event)?;
                }
                self.publish_cancel_writer(&session);
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
                crate::runtime::lifecycle::NewSessionSpec {
                    cwd: &cwd_string,
                    profile_id: &profile.id,
                    vision_capable: client_settings.vision_capable,
                    kind: SessionKind::Chat,
                    resume: None,
                },
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

    /// Switch the visible conversation without killing the agent process:
    /// close the current agent-side session and resume-or-create the target
    /// on the same child. `saved_session` carries the adopted history hint
    /// (`None` means a genuinely fresh thread). Falls back to a full connect
    /// honoring the hint when no live process exists or the close fails.
    pub fn switch_session<F>(
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
        let workspace = resolve_session_cwd(cwd.as_deref())?;
        let cwd_string = workspace.to_string_lossy().into_owned();
        let profile = resolve_active_profile(&prepared, profile_id.as_deref())?;

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;

        if let Some(session) = guard.as_mut() {
            on_event(AcpEvent::Progress {
                message: "正在切换对话…".into(),
            });
            self.host.set_workspace(workspace);
            self.host.release_all();

            if session.init.supports_session_close {
                if let Err(error) = Self::close_agent_session(self, session, &mut on_event) {
                    tracing::warn!(%error, "session/close failed during switch; respawning agent");
                    drop(guard);
                    self.drop_live_session(true);
                    self.clear_cancel_writer();
                    return self.connect(
                        Some(cwd_string),
                        profile_id,
                        saved_session,
                        client_settings,
                        profiles,
                        on_event,
                    );
                }
            }

            let new_session_id = self.open_session_on_live_process(
                session,
                saved_session.as_ref(),
                crate::runtime::lifecycle::NewSessionSpec {
                    cwd: &cwd_string,
                    profile_id: &profile.id,
                    vision_capable: client_settings.vision_capable,
                    kind: SessionKind::Chat,
                    resume: None,
                },
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

            self.publish_cancel_writer(session);
            on_event(AcpEvent::Progress {
                message: "对话已切换".into(),
            });
            Ok(())
        } else {
            drop(guard);
            self.connect(
                cwd,
                profile_id,
                saved_session,
                client_settings,
                profiles,
                on_event,
            )
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
            &session.stdin,
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

/// One neutral transcript turn replayed by `session/load`.
///
/// The Agent streams history as `session/update` notifications
/// (`user_message_chunk` / `agent_message_chunk` / `agent_thought_chunk` /
/// `tool_call*`); the final `session/load` result carries only modes and
/// config options, never text. Roles stay neutral (`user` / `agent` /
/// `tool`) so callers can rebuild local archives without learning ACP
/// update kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedTurn {
    pub role: String,
    pub text: String,
}

impl LoadedTurn {
    fn new(role: &str, text: String) -> Option<Self> {
        if text.trim().is_empty() {
            return None;
        }
        Some(Self {
            role: role.to_string(),
            text,
        })
    }
}

/// Bound for one `session/load` replay (history streams are smaller than a
/// full prompt turn, which gets 600s; large threads still need headroom).
const LOAD_TRANSCRIPT_TIMEOUT_SECS: u64 = 120;

/// Resets `busy` when a transcript load exits on any path.
struct BusyReset<'a>(&'a AtomicBool);

impl Drop for BusyReset<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl AcpService {
    /// Replay a stored thread's real text via `session/load`.
    ///
    /// History panels used to restore only Agent memory (`session/resume`)
    /// and never pulled the thread's actual turns, so the local archive
    /// drifted from the thread. codex-acp implements `session/load`
    /// (`zLoadSessionRequest`: `sessionId` + `cwd` + `mcpServers`) by
    /// resuming the thread and re-streaming every turn as `session/update`
    /// (`getOrCreateSessionWithHistory` + `streamThreadHistory`); the final
    /// result carries no text. This mirrors that: send `session/load` on the
    /// live child and collect the streamed updates until the matching
    /// response id arrives.
    ///
    /// Refuses with `busy` while a prompt (or another load) runs so two
    /// writers never share the child's stdin. Any failure maps to a fixed
    /// business error with the wire detail in `details`. Stream-desync
    /// failures (timeout, EOF, IO error, cancel) drop the live session so
    /// the next prompt spawns clean instead of eating trailing history as
    /// its own reply.
    pub fn load_session_transcript(
        &self,
        session_id: String,
        cwd: Option<String>,
    ) -> Result<Vec<LoadedTurn>, AcpError> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Err(AcpError::bad_request("会话标识不能为空"));
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AcpError::busy());
        }
        let _busy = BusyReset(&self.busy);
        self.cancel.store(false, Ordering::SeqCst);

        let workspace = resolve_session_cwd(cwd.as_deref())?;
        let cwd_string = workspace.to_string_lossy().into_owned();
        let vision_capable = session_env()
            .ok()
            .and_then(|env| env.snapshot_vision_capable(&workspace))
            .unwrap_or(true);
        let env = session_env()?;
        let snapshot_path = env.snapshot_path(&workspace);
        env.sync_snapshot(&snapshot_path, vision_capable)
            .map_err(|error| AcpError::internal(Some(&error)))?;
        let mcp_servers = env.mcp_servers(&snapshot_path, self.isolated_task());

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AcpError::protocol(Some("no active agent session")))?;
        if !session.init.load_session {
            return Err(AcpError::protocol(Some(
                "agent does not advertise session/load",
            )));
        }
        let request_id = session.next_id;
        session.next_id += 1;
        write_request(
            &session.stdin,
            request_id,
            "session/load",
            session_load_params(session_id, &cwd_string, mcp_servers),
        )?;

        let started = Instant::now();
        let deadline = started + Duration::from_secs(LOAD_TRANSCRIPT_TIMEOUT_SECS);
        let mut turns = Vec::new();
        tracing::info!(session_id, cwd = %cwd_string, "ACP session/load started");
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                self.abandon_desynced_session(&mut guard);
                return Err(AcpError::cancelled());
            }
            if Instant::now() > deadline {
                self.abandon_desynced_session(&mut guard);
                tracing::warn!(
                    session_id,
                    elapsed_ms = started.elapsed().as_millis(),
                    "ACP session/load timed out"
                );
                return Err(AcpError::protocol(Some("session/load timed out")));
            }
            let mut line = String::new();
            match session.reader.read_line(&mut line) {
                Ok(0) => {
                    self.abandon_desynced_session(&mut guard);
                    return Err(AcpError::protocol(Some("EOF on stdout")));
                }
                Ok(_) => {}
                Err(error) => {
                    self.abandon_desynced_session(&mut guard);
                    tracing::warn!(%error, "ACP session/load read failed");
                    return Err(AcpError::protocol(Some(&format!(
                        "read ACP stdout: {error}"
                    ))));
                }
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(trimmed) {
                Ok(value) => value,
                Err(error) => {
                    let sample: String = trimmed.chars().take(200).collect();
                    tracing::debug!(line = %sample, %error, "ACP skipping non-JSON stdout line");
                    continue;
                }
            };
            // History fast path: `read_until_id_raw` funnels these through
            // `emit_session_update`, which drops `user_message_chunk` (chat
            // never echoes the user). A shared read would lose every user
            // turn, so transcript collection maps the raw update here.
            if value.get("method").and_then(Value::as_str) == Some("session/update")
                && value.get("id").is_none()
            {
                if let Some(turn) = map_load_update_to_turn(&value) {
                    turns.push(turn);
                }
                continue;
            }
            let mut sink = |_: AcpEvent| {};
            match handle_inbound_side_effects(
                self,
                session,
                &self.host,
                classify_inbound(value),
                &self.cancel,
                &mut sink,
            )? {
                Some((id, response)) if id == request_id => {
                    if let Some(message) = is_error_response(&response) {
                        return Err(AcpError::protocol(Some(&format!(
                            "session/load: {message}"
                        ))));
                    }
                    tracing::info!(
                        session_id,
                        turns = turns.len(),
                        elapsed_ms = started.elapsed().as_millis(),
                        "ACP session/load completed"
                    );
                    return Ok(turns);
                }
                Some(_) | None => {}
            }
        }
    }

    /// Take + terminate the live child after a stream-desync failure
    /// (timeout, EOF, IO error, cancel): the Agent may still be streaming
    /// history afterwards, and that backlog would otherwise be eaten as the
    /// next prompt's reply. Callers hold the session lock; this never blocks.
    fn abandon_desynced_session(
        &self,
        guard: &mut std::sync::MutexGuard<'_, Option<crate::runtime::lifecycle::LiveSession>>,
    ) {
        if let Some(mut taken) = guard.take() {
            self.clear_cancel_writer();
            taken.agent.terminate(false);
        }
    }
}

/// Map one streamed `session/update` notification to a neutral transcript turn.
///
/// Accepts the full notification (`method` + `params.update`) as well as a
/// bare update object so the mapping stays unit-testable without a child.
/// Empty bodies map to `None` (status-only tool updates, blank chunks).
/// `agent_thought_chunk` folds into `agent`: reasoning is agent text and the
/// DTO only knows `user` / `agent` / `tool`.
pub(crate) fn map_load_update_to_turn(value: &Value) -> Option<LoadedTurn> {
    let update = load_update(value)?;
    match update.get("sessionUpdate").and_then(Value::as_str)? {
        "user_message_chunk" => LoadedTurn::new("user", load_content_text(update.get("content")?)?),
        "agent_message_chunk" | "agent_thought_chunk" => {
            LoadedTurn::new("agent", load_content_text(update.get("content")?)?)
        }
        "tool_call" | "tool_call_update" => {
            let tool = extract_tool_call(value)?;
            let text = tool
                .detail
                .filter(|text| !text.trim().is_empty())
                .or_else(|| tool.title.filter(|text| !text.trim().is_empty()))?;
            LoadedTurn::new("tool", text)
        }
        "tool_call_content_chunk" => {
            let (_, detail) = extract_tool_call_content_chunk(value)?;
            LoadedTurn::new("tool", detail)
        }
        "plan" => LoadedTurn::new("agent", extract_plan_summary(value)?),
        _ => None,
    }
}

fn load_update(value: &Value) -> Option<&Value> {
    if let Some(update) = value.pointer("/params/update") {
        if update.is_object() {
            return Some(update);
        }
    }
    if value.get("sessionUpdate").is_some() {
        return Some(value);
    }
    None
}

/// Plain text out of an ACP content block: bare string, `{type:text}`,
/// arrays of blocks, and `resource_link` (rendered like codex-acp history
/// replay: `[@name](uri)`).
fn load_content_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    if content.get("type").and_then(Value::as_str) == Some("resource_link") {
        let uri = content.get("uri").and_then(Value::as_str).unwrap_or("");
        if uri.trim().is_empty() {
            return None;
        }
        let name = content
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(uri);
        return Some(format!("[@{name}]({uri})"));
    }
    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    let blocks = content.as_array()?;
    let mut parts = Vec::new();
    for item in blocks {
        if let Some(text) = load_content_text(item) {
            if !text.trim().is_empty() {
                parts.push(text);
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::profile::AgentKind;
    use std::process::Stdio;

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
            agent: crate::runtime::process::AgentProcess::adopt(child),
            stdin: std::sync::Arc::new(std::sync::Mutex::new(stdin)),
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

    /// A slot session whose process stays alive and never answers: stdin
    /// writes succeed, stdout never produces a response line.
    fn silent_slot_session() -> crate::runtime::lifecycle::LiveSession {
        use std::io::BufReader;
        let mut child = crate::runtime::process::command("cmd")
            .args(["/c", "ping", "-n", "30", "127.0.0.1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn silent agent");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        crate::runtime::lifecycle::LiveSession {
            agent: crate::runtime::process::AgentProcess::adopt(child),
            stdin: std::sync::Arc::new(std::sync::Mutex::new(stdin)),
            reader: BufReader::new(stdout),
            session_id: "silent-session".to_string(),
            next_id: 1,
            init: crate::wire::session::InitializeResult {
                supports_session_close: true,
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
    fn switch_session_refuses_while_busy_without_side_effects() {
        use std::sync::atomic::Ordering;

        let service = AcpService::new();
        service.busy.store(true, Ordering::SeqCst);
        let err = service
            .switch_session(
                None,
                None,
                None,
                AcpClientSettings::default(),
                crate::domain::model::AgentProfilesHint {
                    active_profile_id: String::new(),
                    profiles: Vec::new(),
                },
                &mut |_| {},
            )
            .expect_err("busy switch must fail");
        assert_eq!(err.code, crate::AcpErrorCode::Busy);
        assert!(!slot_occupied(&service));
        assert!(!service.cancel.load(Ordering::SeqCst));
    }

    /// Cancel must preempt a prompt blocked in `read_line`: the prompt loop
    /// holds the session mutex across the blocking read, so `request_cancel`
    /// may only use the mirrored writer there. Holding the session lock here
    /// simulates that blocked prompt; on the old path this test deadlocks.
    #[test]
    fn request_cancel_does_not_wait_for_session_lock() {
        let service = AcpService::new();
        let session = silent_slot_session();
        service.publish_cancel_writer(&session);
        let mut guard = service.session.lock().expect("lock");
        *guard = Some(session);
        // `guard` stays held like the prompt loop across `read_line`.
        let elapsed = std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                let started = std::time::Instant::now();
                service.request_cancel();
                started.elapsed()
            });
            handle.join().expect("cancel thread")
        });
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "cancel must not wait on the session lock: {elapsed:?}"
        );
        assert!(service.cancel.load(std::sync::atomic::Ordering::SeqCst));
        drop(guard);
        service.drop_live_session(false);
    }

    /// Closing gives the agent a grace period to persist the conversation, but
    /// must never wait for it: `read_one` blocks in `read_line`, so an awaited
    /// handshake would freeze media switching whenever the agent stays quiet.
    #[test]
    fn closing_a_silent_session_does_not_block_the_caller() {
        let service = AcpService::new_isolated(None);
        service
            .session
            .lock()
            .expect("lock")
            .replace(silent_slot_session());

        let started = std::time::Instant::now();
        service.drop_live_session(true);

        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "close must not wait on the agent"
        );
        assert!(!slot_occupied(&service));
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
    fn load_transcript_timeout_is_locked() {
        // History replay bound: large threads need headroom, but a wedged
        // agent must fail loudly instead of hanging the history panel.
        assert_eq!(LOAD_TRANSCRIPT_TIMEOUT_SECS, 120);
    }

    #[test]
    fn load_session_transcript_refuses_while_busy_without_side_effects() {
        use std::sync::atomic::Ordering;

        let service = AcpService::new();
        service.busy.store(true, Ordering::SeqCst);
        let err = service
            .load_session_transcript("sess-1".to_string(), None)
            .expect_err("busy load must fail");
        assert_eq!(err.code, crate::AcpErrorCode::Busy);
        assert!(!service.cancel.load(Ordering::SeqCst));
    }

    #[test]
    fn load_session_transcript_rejects_blank_session_id() {
        let service = AcpService::new();
        let err = service
            .load_session_transcript("   ".to_string(), None)
            .expect_err("blank id must fail");
        assert_eq!(err.code, crate::AcpErrorCode::ProtocolError);
        assert!(err.message.contains("会话标识"));
    }

    #[test]
    fn load_transcript_mapping_covers_user_agent_tool_and_skips_empty() {
        use serde_json::json;

        // Shapes mirror codex-acp history replay (`createUserMessageChunk` /
        // `createAgentMessageChunk` / tool_call updates over `session/update`).
        let user = json!({
            "method": "session/update",
            "params": {
                "sessionId": "sess-1",
                "update": {
                    "sessionUpdate": "user_message_chunk",
                    "content": { "type": "text", "text": "这段讲了什么？" },
                },
            },
        });
        assert_eq!(
            map_load_update_to_turn(&user),
            Some(LoadedTurn {
                role: "user".to_string(),
                text: "这段讲了什么？".to_string(),
            })
        );

        let agent = json!({
            "method": "session/update",
            "params": {
                "sessionId": "sess-1",
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": [
                        { "type": "text", "text": "本集讲了" },
                        { "type": "text", "text": "重逢。" },
                    ],
                },
            },
        });
        assert_eq!(
            map_load_update_to_turn(&agent),
            Some(LoadedTurn {
                role: "agent".to_string(),
                text: "本集讲了重逢。".to_string(),
            })
        );

        // Reasoning folds into `agent`: the DTO only knows user/agent/tool.
        let thought = json!({
            "sessionUpdate": "agent_thought_chunk",
            "content": { "type": "text", "text": "先查台词再回答" },
        });
        assert_eq!(
            map_load_update_to_turn(&thought)
                .as_ref()
                .map(|turn| turn.role.as_str()),
            Some("agent")
        );

        let tool = json!({
            "method": "session/update",
            "params": {
                "sessionId": "sess-1",
                "update": {
                    "sessionUpdate": "tool_call",
                    "toolCallId": "call-1",
                    "title": "搜索",
                    "status": "completed",
                    "content": [
                        {
                            "type": "content",
                            "content": { "type": "text", "text": "找到 3 条台词" },
                        },
                    ],
                },
            },
        });
        assert_eq!(
            map_load_update_to_turn(&tool),
            Some(LoadedTurn {
                role: "tool".to_string(),
                text: "找到 3 条台词".to_string(),
            })
        );

        // Empty bodies never become turns: blank agent chunk, status-only
        // tool update, and metadata updates carry no transcript text.
        for empty in [
            json!({
                "method": "session/update",
                "params": {
                    "update": {
                        "sessionUpdate": "agent_message_chunk",
                        "content": { "type": "text", "text": "   " },
                    },
                },
            }),
            json!({
                "method": "session/update",
                "params": {
                    "update": {
                        "sessionUpdate": "tool_call_update",
                        "toolCallId": "call-1",
                        "status": "completed",
                    },
                },
            }),
            json!({
                "method": "session/update",
                "params": {
                    "update": {
                        "sessionUpdate": "session_info_update",
                        "title": "看剧对话",
                    },
                },
            }),
            json!({ "id": 7, "result": {} }),
        ] {
            assert_eq!(map_load_update_to_turn(&empty), None, "value={empty}");
        }
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
}
