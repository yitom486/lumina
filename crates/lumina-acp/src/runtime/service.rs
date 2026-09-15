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
use std::time::Duration;

use crate::agent::profile::{prepare_profiles, resolve_active_profile};
use crate::agent::status::status_from_profiles;
use crate::agent::workspace::resolve_session_cwd;
use crate::domain::environment::session_env;
use crate::domain::model::{
    AcpEvent, AcpModelDiscoveryResult, AcpSessionModelOptions, AcpSessionModelSelection, AcpStatus,
    AgentProfilesHint, SavedSessionHint,
};
use crate::domain::settings::{AcpClientSettings, PermissionMode};
use crate::error::AcpError;
use crate::runtime::host::AcpHost;
use crate::wire::codec::is_error_response;
use crate::wire::session::session_cancel_params;

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
}
