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
use crate::runtime::io::{read_until_id_raw, write_request};
use crate::wire::codec::{classify_inbound, is_error_response};
use crate::wire::session::{session_cancel_params, session_delete_params, session_load_params};

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
            // live 槽是单例，但归属是按 profile 的：别家还活着不等于自家
            // 已连接。以前这里不区分，切画像后前端会把旧 Agent 的活着误认
            // 成新 Agent 已连，直接跳过重连（绿 badge 常亮、实际没连上）。
            status.session_active = guard
                .as_ref()
                .is_some_and(|session| session.profile_id == status.active_profile_id);
            // Model options describe one agent's live session. Reporting them
            // for a different active profile made the composer show, say,
            // Codex GPT models while another agent was connecting.
            status.session_model_options = guard
                .as_ref()
                .filter(|session| session.profile_id == status.active_profile_id)
                .map(|session| session.model_options.clone());
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
                &env.snapshot_path(&workspace, SessionKind::Chat),
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
            // A live agent process cannot serve another profile: prompts,
            // history and models would all keep hitting the old ACP server.
            // Wind it down (graceful, rollout-flush preserving) and let the
            // full connect spawn the newly active agent.
            if session.profile_id != profile.id {
                on_event(AcpEvent::Progress {
                    message: format!("正在切换到 {}…", profile.name),
                });
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

    /// Apply one advertised session config option (cursor mode/model/effort/
    /// context/fast …) to the live session without rotating it.
    /// The value MUST come from the agent's advertised list: synthesized ids
    /// are rejected with `Invalid params`. Cached `current` is updated only
    /// after the agent accepts the write.
    pub fn set_session_config<F>(
        &self,
        config_id: String,
        value: String,
        mut on_event: F,
    ) -> Result<AcpSessionModelOptions, AcpError>
    where
        F: FnMut(AcpEvent),
    {
        if self.is_busy() {
            return Err(AcpError::busy());
        }
        let config_id = config_id.trim().to_string();
        if config_id.is_empty() {
            return Err(AcpError::bad_request("配置项 id 不能为空"));
        }

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AcpError::protocol(Some("no active agent session")))?;

        let response = Self::set_session_config_option(
            session,
            &config_id,
            value.trim(),
            self,
            &mut on_event,
        )?;
        // 服务端回包自带最新 configOptions（cursor 实测有）：以它为准刷新
        // 缓存；缺席则回退乐观更新（只改 current），不丢本地状态。
        let authoritative = crate::wire::session::parse_session_model_options(&response);
        if !authoritative.extra_options.is_empty() {
            session.model_options = authoritative;
        } else {
            for option in &mut session.model_options.extra_options {
                if option.id != config_id {
                    continue;
                }
                match &mut option.kind {
                    crate::domain::model::SessionConfigKind::Select { current, .. } => {
                        *current = Some(value.trim().to_string());
                    }
                    crate::domain::model::SessionConfigKind::Boolean { current } => {
                        *current = value.trim().eq_ignore_ascii_case("true");
                    }
                    crate::domain::model::SessionConfigKind::Unsupported => {}
                }
            }
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
    ) -> Result<Value, AcpError> {
        // cursor 切模型实测 4~8s、尾部更长；60s 仍不够才报超时。
        // 超时只重试一次：同值重发幂等，且每次用新 id，老响应按 id 错过，
        // 不会串台。
        let mut attempt = 0;
        loop {
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
            match crate::runtime::io::read_until_id_raw(
                service,
                session,
                request_id,
                Duration::from_secs(60),
                &service.cancel,
                &service.host,
                on_event,
            ) {
                Err(error) if attempt == 0 && is_read_timeout(&error) => {
                    attempt += 1;
                    tracing::warn!(
                        config_id,
                        "session/set_config_option timed out; retrying once with a fresh id"
                    );
                    continue;
                }
                Err(error) => return Err(error),
                Ok(response) => {
                    if let Some(message) = is_error_response(&response) {
                        return Err(AcpError::protocol(Some(&format!(
                            "session config {config_id}: {message}"
                        ))));
                    }
                    return Ok(response);
                }
            }
        }
    }
}

/// True only for our own read timeout (not agent error responses, not
/// cancel, not EOF): the only case where a same-value retry is safe.
fn is_read_timeout(error: &AcpError) -> bool {
    error.code == crate::AcpErrorCode::ProtocolError
        && error.details.as_deref() == Some("ACP read timed out")
}

impl Default for AcpService {
    fn default() -> Self {
        Self::new()
    }
}

/// One public, completed transcript message replayed by `session/load`.
///
/// ACP history streams raw execution updates, but callers must never learn
/// about thoughts, plans, tool calls, or implementation details. The loader
/// projects that stream into neutral public messages (`user` / `agent`) only.
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

/// Avoid letting a malformed or unexpectedly large remote history retain an
/// unbounded amount of display text in the desktop process.
const MAX_LOADED_TRANSCRIPT_TURNS: usize = 2_000;
const MAX_LOADED_TRANSCRIPT_MESSAGE_CHARS: usize = 64 * 1024;

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
        profile_id: Option<String>,
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

        // Refuse a cross-profile load before any workspace/snapshot work.
        {
            let guard = self
                .session
                .lock()
                .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
            if let Some(session) = guard.as_ref() {
                if let Some(requested) = profile_id.as_deref().filter(|id| !id.trim().is_empty()) {
                    if session.profile_id != requested {
                        return Err(AcpError::bad_request(
                            "该对话属于其他 Agent，请先切换到对应 Agent 再查看",
                        ));
                    }
                }
            }
        }

        let workspace = resolve_session_cwd(cwd.as_deref())?;
        let cwd_string = workspace.to_string_lossy().into_owned();
        let vision_capable = session_env()
            .ok()
            .and_then(|env| env.snapshot_vision_capable(&workspace))
            .unwrap_or(true);
        let env = session_env()?;
        let snapshot_path = env.snapshot_path(&workspace, SessionKind::Chat);
        env.sync_snapshot(&snapshot_path, vision_capable)
            .map_err(|error| AcpError::internal(Some(&error)))?;
        let mcp_servers = env.mcp_servers(&snapshot_path, SessionKind::Chat);

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
        let mut projector = LoadTranscriptProjector::default();
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
                projector.push(&value);
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
                        // This path used to be silent: the UI reported a load
                        // failure with no backend trace of the agent's reason.
                        tracing::warn!(
                            session_id,
                            turns = projector.public_turn_count(),
                            elapsed_ms = started.elapsed().as_millis(),
                            %message,
                            "ACP session/load failed"
                        );
                        return Err(AcpError::protocol(Some(&format!(
                            "session/load: {message}"
                        ))));
                    }
                    let turns = projector.finish();
                    let mut user_turns = 0usize;
                    let mut agent_turns = 0usize;
                    for turn in &turns {
                        match turn.role.as_str() {
                            "user" => user_turns += 1,
                            "agent" => agent_turns += 1,
                            _ => {}
                        }
                    }
                    tracing::info!(
                        session_id,
                        turns = turns.len(),
                        user_turns,
                        agent_turns,
                        // Role pairing without content: shows whether user
                        // content landed in agent bubbles and vice versa.
                        turns_detail = %turns
                            .iter()
                            .map(|turn| {
                                let kind = turn.role.chars().next().unwrap_or('?');
                                format!("{kind}:{}", turn.text.len())
                            })
                            .collect::<Vec<_>>()
                            .join(","),
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

    /// Delete a stored thread via native `session/delete`.
    ///
    /// Refuses while a prompt (or load) runs: delete shares the child's
    /// single stdio stream. Refuses the currently attached session id:
    /// deleting the live attachment from under the next prompt would
    /// strand it. Callers switch away (or start fresh) first.
    pub fn delete_session(
        &self,
        profile_id: Option<String>,
        session_id: String,
    ) -> Result<(), AcpError> {
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

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AcpError::protocol(Some("no active agent session")))?;
        if let Some(requested) = profile_id.as_deref().filter(|id| !id.trim().is_empty()) {
            if session.profile_id != requested {
                return Err(AcpError::bad_request(
                    "该对话属于其他 Agent，请先切换到对应 Agent 再删除",
                ));
            }
        }
        if !session.init.supports_session_delete {
            return Err(AcpError::protocol(Some(
                "agent does not advertise session/delete",
            )));
        }
        if session.session_id == session_id {
            return Err(AcpError::bad_request("不能删除正在使用的对话"));
        }
        let request_id = session.next_id;
        session.next_id += 1;
        write_request(
            &session.stdin,
            request_id,
            "session/delete",
            session_delete_params(session_id),
        )?;
        let started = Instant::now();
        let mut sink = |_: AcpEvent| {};
        let response = read_until_id_raw(
            self,
            session,
            request_id,
            Duration::from_secs(30),
            &self.cancel,
            &self.host,
            &mut sink,
        )?;
        if let Some(message) = is_error_response(&response) {
            tracing::warn!(
                session_id,
                elapsed_ms = started.elapsed().as_millis(),
                %message,
                "ACP session/delete failed"
            );
            return Err(AcpError::protocol(Some(&format!(
                "session/delete: {message}"
            ))));
        }
        tracing::info!(
            session_id,
            elapsed_ms = started.elapsed().as_millis(),
            "ACP session/delete completed"
        );
        Ok(())
    }
}

/// Projects raw `session/update` notifications into only the messages that
/// are safe to restore in a user's conversation. It deliberately keeps no
/// ACP thought, plan, or tool content. An internal update invalidates any
/// earlier provisional agent text in the same user turn, so an Agent that
/// writes a short preamble before using tools cannot leak it as the answer.
#[derive(Default)]
struct LoadTranscriptProjector {
    turns: Vec<LoadedTurn>,
    current: Option<PendingLoadedTurn>,
}

#[derive(Default)]
struct PendingLoadedTurn {
    user_text: Option<String>,
    agent_text: String,
}

impl LoadTranscriptProjector {
    fn public_turn_count(&self) -> usize {
        self.turns.len() + usize::from(self.current.is_some())
    }

    fn push(&mut self, value: &Value) {
        let Some(update) = load_update(value) else {
            return;
        };
        let Some(kind) = update.get("sessionUpdate").and_then(Value::as_str) else {
            return;
        };

        match kind {
            "user_message_chunk" => {
                let Some(text) = update.get("content").and_then(load_public_user_content) else {
                    return;
                };
                self.finish_current();
                self.current = Some(PendingLoadedTurn {
                    user_text: Some(text),
                    agent_text: String::new(),
                });
            }
            "agent_message_chunk" => {
                let Some(text) = update.get("content").and_then(load_content_text) else {
                    return;
                };
                if text.trim().is_empty() {
                    return;
                }
                let current = self.current.get_or_insert_with(PendingLoadedTurn::default);
                append_bounded(&mut current.agent_text, &text);
            }
            // These are execution-internal updates, never public history. If
            // any follows an agent preamble, that preamble was provisional;
            // the later agent message (if present) is the answer to restore.
            "agent_thought_chunk"
            | "plan"
            | "tool_call"
            | "tool_call_update"
            | "tool_call_content_chunk" => {
                if let Some(current) = self.current.as_mut() {
                    current.agent_text.clear();
                }
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Vec<LoadedTurn> {
        self.finish_current();
        self.turns
    }

    fn finish_current(&mut self) {
        let Some(current) = self.current.take() else {
            return;
        };
        let emits_user = current.user_text.is_some();
        let emits_agent = !current.agent_text.trim().is_empty();
        let needed = usize::from(emits_user) + usize::from(emits_agent);
        if self.turns.len().saturating_add(needed) > MAX_LOADED_TRANSCRIPT_TURNS {
            return;
        }
        if let Some(user_text) = current.user_text {
            self.push_turn("user", user_text);
        }
        self.push_turn("agent", current.agent_text);
    }

    fn push_turn(&mut self, role: &str, text: String) {
        if self.turns.len() >= MAX_LOADED_TRANSCRIPT_TURNS {
            return;
        }
        if let Some(turn) = LoadedTurn::new(role, text) {
            self.turns.push(turn);
        }
    }
}

fn append_bounded(target: &mut String, text: &str) {
    let used = target.chars().count();
    if used >= MAX_LOADED_TRANSCRIPT_MESSAGE_CHARS {
        return;
    }
    target.extend(
        text.chars()
            .take(MAX_LOADED_TRANSCRIPT_MESSAGE_CHARS.saturating_sub(used)),
    );
}

const LUMINA_PROMPT_MARKERS: [&str; 2] = ["【工具优先】", "【当前播放】"];
const LUMINA_CONTEXT_PREFIXES: [&str; 20] = [
    "媒体：",
    "媒体:",
    "进度：",
    "进度:",
    "时长：",
    "时长:",
    "集数：",
    "集数:",
    "季数：",
    "季数:",
    "字幕轨道：",
    "字幕轨道:",
    "本集标题：",
    "本集标题:",
    "本集剧情：",
    "本集剧情:",
    "台词上下文窗口建议：",
    "台词上下文窗口建议:",
    "台词上下文：",
    "台词上下文:",
];

/// Remove the prompt scaffolding Lumina injects before the real user text.
/// This belongs at the ACP history boundary, not in a React string cleaner:
/// a restored transcript must already contain public content only.
fn sanitize_loaded_user_text(text: &str) -> Option<String> {
    let kept = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.is_empty()
                || LUMINA_PROMPT_MARKERS
                    .iter()
                    .any(|marker| trimmed.starts_with(marker))
                || LUMINA_CONTEXT_PREFIXES
                    .iter()
                    .any(|prefix| trimmed.starts_with(prefix))
                || trimmed.contains("file://"))
        })
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    (!kept.is_empty()).then_some(kept)
}

fn load_public_user_content(content: &Value) -> Option<String> {
    if let Some(blocks) = content.as_array() {
        let text = blocks
            .iter()
            .filter_map(load_content_text)
            .filter_map(|text| sanitize_loaded_user_text(&text))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        return (!text.is_empty()).then_some(text);
    }
    load_content_text(content).and_then(|text| sanitize_loaded_user_text(&text))
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
    use crate::agent::profile::CODEX_EMPTY_REPLY_HINT;
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
            profile_id: "codex".to_string(),
            next_id: 1,
            init: crate::wire::session::InitializeResult {
                supports_session_close,
                ..crate::wire::session::InitializeResult::default()
            },
            empty_reply_hint: CODEX_EMPTY_REPLY_HINT.to_string(),
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
            profile_id: "codex".to_string(),
            next_id: 1,
            init: crate::wire::session::InitializeResult {
                supports_session_close: true,
                ..crate::wire::session::InitializeResult::default()
            },
            empty_reply_hint: CODEX_EMPTY_REPLY_HINT.to_string(),
            model_options: AcpSessionModelOptions::default(),
        }
    }

    fn slot_occupied(service: &AcpService) -> bool {
        service.session.lock().expect("lock").is_some()
    }

    #[test]
    fn status_hides_model_options_of_another_profiles_live_session() {
        let service = AcpService::new();
        let mut session = dead_slot_session(true);
        session.model_options.models.push(crate::AcpSessionOption {
            value: "gpt-5.6".into(),
            name: "GPT-5.6".into(),
            description: None,
        });
        *service.session.lock().expect("lock") = Some(session);

        // Live session belongs to codex: options visible while codex is active…
        let codex_hint = crate::agent::profile::default_profiles_hint();
        let status = service.status(&codex_hint);
        assert!(status.session_model_options.is_some());
        assert!(status.session_active);

        // …but hidden the moment another profile becomes active — and the
        // session no longer counts as active either (a live foreign process
        // must never read as "connected" for the new agent, otherwise the
        // client skips reconnecting after a profile switch).
        let mut other_hint = codex_hint;
        other_hint.active_profile_id = "claude".into();
        let status = service.status(&other_hint);
        assert!(status.session_model_options.is_none());
        assert!(!status.session_active);
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

    #[test]
    fn delete_session_refuses_while_busy_without_side_effects() {
        use std::sync::atomic::Ordering;

        let service = AcpService::new();
        service.busy.store(true, Ordering::SeqCst);
        let err = service
            .delete_session(None, "sess-1".to_string())
            .expect_err("busy delete must fail");
        assert_eq!(err.code, crate::AcpErrorCode::Busy);
        assert!(!slot_occupied(&service));
        // Busy stays set: the holder is whoever set it, not us.
        assert!(service.busy.load(Ordering::SeqCst));
    }

    #[test]
    fn delete_session_rejects_blank_session_id() {
        let service = AcpService::new();
        let err = service
            .delete_session(None, "   ".to_string())
            .expect_err("blank id must fail");
        // `bad_request` carries a specific Chinese message on ProtocolError.
        assert_eq!(err.code, crate::AcpErrorCode::ProtocolError);
        assert!(err.message.contains("不能为空"));
    }

    #[test]
    fn list_agent_sessions_reports_unverified_for_other_profile() {
        let service = AcpService::new();
        let mut session = dead_slot_session(true);
        session.init.supports_session_list = true;
        *service.session.lock().expect("lock") = Some(session);

        // The live agent is codex; asking as claude must not surface
        // codex history under the claude scope.
        let result = service
            .list_agent_sessions(Some("claude"), None)
            .expect("mismatch degrades to unverified");
        assert!(!result.verified);
        assert!(result.sessions.is_empty());
        assert!(!result.truncated);
    }

    #[test]
    fn cross_profile_delete_is_refused() {
        let service = AcpService::new();
        *service.session.lock().expect("lock") = Some(dead_slot_session(true));

        let err = service
            .delete_session(Some("claude".into()), "sess-1".into())
            .expect_err("cross-profile delete refused");
        assert!(err.message.contains("其他 Agent"));

        // Same profile passes the gate (then hits the dead transport).
        let err = service
            .delete_session(Some("codex".into()), "sess-1".into())
            .expect_err("dead stdin errors");
        assert_eq!(err.code, crate::AcpErrorCode::ProtocolError);
    }

    #[test]
    fn cross_profile_load_is_refused() {
        let service = AcpService::new();
        *service.session.lock().expect("lock") = Some(dead_slot_session(true));

        let err = service
            .load_session_transcript(Some("claude".into()), "sess-1".into(), None)
            .expect_err("cross-profile load refused");
        assert!(err.message.contains("其他 Agent"));
    }

    /// 专用回归：activeProfile 变化时，底层所有 agent 相关状态必须整体跟随。
    ///
    /// 挂着一个 Codex live 会话，把 active profile 切到另一个 Agent，逐项断言：
    /// status 不再透出旧 Agent 的模型选项；session/list / load / delete 都拒绝
    /// 服务旧 Agent；switch/prompt 会把旧 Agent 进程退场（slot 清空）。任何一处
    /// 还认旧 Agent，这个测试就红。
    fn foreign_profile_hint(profile_id: &str, command: &str) -> AgentProfilesHint {
        let mut hint = crate::agent::profile::default_profiles_hint();
        hint.active_profile_id = profile_id.into();
        hint.profiles.push(crate::AgentProfileInput {
            id: profile_id.into(),
            name: "Test Agent".into(),
            kind: crate::AgentKind::Custom,
            command: command.into(),
            args: Vec::new(),
            env: Default::default(),
            launcher: None,
            env_preset: None,
            auth_policy: None,
            auth_methods: Vec::new(),
            session_storage: None,
        });
        hint
    }

    #[test]
    fn active_profile_change_propagates_to_every_agent_scoped_state() {
        let hint = foreign_profile_hint("ghost", "definitely-not-here-xyz-ghost");
        let service = AcpService::new();
        let mut session = dead_slot_session(true);
        session.init.supports_session_list = true;
        session.init.supports_session_delete = true;
        session.init.load_session = true;
        session.model_options.models.push(crate::AcpSessionOption {
            value: "gpt-5.6".into(),
            name: "GPT-5.6".into(),
            description: None,
        });
        *service.session.lock().expect("lock") = Some(session);

        // 1. status: 旧 Agent 的模型选项必须立刻消失；旧进程还活着，
        // 但对 ghost 而言没有可用会话（session_active 同样按 profile 收敛，
        // 否则前端会把旧 Agent 的活着误认成新 Agent 已连，跳过重连）。
        let status = service.status(&hint);
        assert!(!status.session_active);
        assert!(status.session_model_options.is_none());

        // 2. session/list 不把 Codex 会话当作 ghost 的历史。
        let list = service
            .list_agent_sessions(Some("ghost"), None)
            .expect("mismatch degrades to unverified");
        assert!(!list.verified);
        assert!(list.sessions.is_empty());

        // 3. load / delete 拒绝服务旧 Agent。
        let err = service
            .load_session_transcript(Some("ghost".into()), "sess-1".into(), None)
            .expect_err("cross-profile load refused");
        assert!(err.message.contains("其他 Agent"));
        let err = service
            .delete_session(Some("ghost".into()), "sess-1".into())
            .expect_err("cross-profile delete refused");
        assert!(err.message.contains("其他 Agent"));

        // 4. 切换：旧 Agent 进程退场（slot 清空），再按新 profile spawn（此处
        //    ghost 不可用 → NotConfigured，但状态已归属新 profile）。
        let err = service
            .switch_session(
                None,
                Some("ghost".into()),
                None,
                crate::AcpClientSettings::default(),
                hint.clone(),
                |_| {},
            )
            .expect_err("ghost agent unavailable");
        assert_eq!(err.code, crate::AcpErrorCode::NotConfigured);
        assert!(!slot_occupied(&service));
        let status = service.status(&hint);
        assert!(!status.session_active);
        assert!(status.session_model_options.is_none());

        // 5. prompt 同理：不会在旧 Agent 进程上继续提问。
        *service.session.lock().expect("lock") = Some(dead_slot_session(true));
        let err = service
            .prompt(
                "hi",
                None,
                Some("ghost".into()),
                None,
                Vec::new(),
                None,
                crate::AcpClientSettings::default(),
                hint,
                |_| {},
            )
            .expect_err("ghost agent unavailable");
        assert_eq!(err.code, crate::AcpErrorCode::NotConfigured);
        assert!(!slot_occupied(&service));
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
            .load_session_transcript(None, "sess-1".to_string(), None)
            .expect_err("busy load must fail");
        assert_eq!(err.code, crate::AcpErrorCode::Busy);
        assert!(!service.cancel.load(Ordering::SeqCst));
    }

    #[test]
    fn load_session_transcript_rejects_blank_session_id() {
        let service = AcpService::new();
        let err = service
            .load_session_transcript(None, "   ".to_string(), None)
            .expect_err("blank id must fail");
        assert_eq!(err.code, crate::AcpErrorCode::ProtocolError);
        assert!(err.message.contains("会话标识"));
    }

    #[test]
    fn load_transcript_projection_keeps_only_completed_public_turns() {
        use serde_json::json;

        let final_answer =
            r#"{"version":"plot_summary.v1","summary":"延秀和崔雄在重逢后仍然互相试探。"}"#;
        let updates = vec![
            json!({ "method": "session/update", "params": { "sessionId": "sess-1", "update": {
                "sessionUpdate": "user_message_chunk",
                "content": [
                    { "type": "text", "text": "【工具优先】本轮优先使用 Lumina 本地工具：lumina_get_transcript_window。" },
                    { "type": "resource_link", "name": "episode.mp4", "uri": "file:///D:/movie/episode.mp4" },
                    { "type": "text", "text": "【当前播放】\n媒体：episode.mp4\n进度：03:32 / 59:34（212337ms）\n字幕轨道：cache:subdl:en\n台词上下文窗口建议：当前播放点前后各 30 秒；读取当前台词时优先使用该范围。" },
                    { "type": "text", "text": "\n\n请帮我梳理当前剧情。" }
                ]
            }}}),
            json!({ "method": "session/update", "params": { "update": {
                "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "我先读取字幕和画面。" }
            }}}),
            json!({ "method": "session/update", "params": { "update": {
                "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "Designing JSON schema for plot summary" }
            }}}),
            json!({ "method": "session/update", "params": { "update": {
                "sessionUpdate": "plan", "entries": [{ "content": "Confirming timestamp boundaries" }]
            }}}),
            json!({ "method": "session/update", "params": { "update": {
                "sessionUpdate": "tool_call", "toolCallId": "call-1", "title": "lumina_get_transcript_window",
                "content": [{ "type": "content", "content": { "type": "text", "text": "D:/movie/private.srt\nstderr: unavailable" }}]
            }}}),
            json!({ "method": "session/update", "params": { "update": {
                "sessionUpdate": "tool_call_update", "toolCallId": "call-1", "status": "completed", "detail": "raw tool result"
            }}}),
            json!({ "method": "session/update", "params": { "update": {
                "sessionUpdate": "tool_call_content_chunk", "toolCallId": "call-1", "content": { "type": "text", "text": "internal tool output" }
            }}}),
            json!({ "method": "session/update", "params": { "update": {
                "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": final_answer }
            }}}),
        ];

        let mut projector = LoadTranscriptProjector::default();
        for update in &updates {
            projector.push(update);
        }
        let turns = projector.finish();
        assert_eq!(
            turns,
            vec![
                LoadedTurn {
                    role: "user".into(),
                    text: "请帮我梳理当前剧情。".into()
                },
                LoadedTurn {
                    role: "agent".into(),
                    text: final_answer.into()
                },
            ]
        );
        let restored = turns
            .iter()
            .map(|turn| turn.text.as_str())
            .collect::<String>();
        for leaked in [
            "台词上下文窗口建议",
            "Designing JSON schema",
            "Confirming timestamp boundaries",
            "lumina_get_transcript_window",
            "private.srt",
            "我先读取字幕和画面",
        ] {
            assert!(!restored.contains(leaked), "leaked {leaked}");
        }
    }

    #[test]
    fn load_transcript_projection_keeps_normal_direct_agent_reply() {
        use serde_json::json;

        let mut projector = LoadTranscriptProjector::default();
        for update in [
            json!({ "sessionUpdate": "user_message_chunk", "content": { "type": "text", "text": "你好" } }),
            json!({ "sessionUpdate": "agent_message_chunk", "content": [
                { "type": "text", "text": "你好，" },
                { "type": "text", "text": "很高兴见到你。" }
            ]}),
        ] {
            projector.push(&update);
        }
        assert_eq!(
            projector.finish(),
            vec![
                LoadedTurn {
                    role: "user".into(),
                    text: "你好".into()
                },
                LoadedTurn {
                    role: "agent".into(),
                    text: "你好，很高兴见到你。".into()
                },
            ]
        );
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
