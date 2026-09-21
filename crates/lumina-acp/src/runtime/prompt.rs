//! Chat prompt flow: `prompt` / `prompt_with_label` / `run_prompt_inner`.
//!
//! Pure move from `runtime/service.rs` (no behavior change).

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::agent::profile::{prepare_profiles, resolve_active_profile, PreparedProfiles};
use crate::agent::workspace::resolve_session_cwd;
use crate::domain::context::VideoPromptContext;
use crate::domain::environment::session_env;
use crate::domain::model::{
    validate_prompt_images, AcpEvent, AgentExecutionRequest, AgentProfilesHint, PromptImage,
    SavedSessionHint, SessionKind,
};
use crate::domain::settings::AcpClientSettings;
use crate::error::AcpError;
use crate::jobs::collector::AgentReplyCollector;
use crate::runtime::service::AcpService;
use crate::wire::codec::is_error_response;
use crate::wire::session::{parse_stop_reason, session_cancel_params};

impl AcpService {
    // This public boundary mirrors the explicit ACP/Tauri request fields.
    #[allow(clippy::too_many_arguments)]
    pub fn prompt<F>(
        &self,
        text: impl AsRef<str>,
        cwd: Option<String>,
        profile_id: Option<String>,
        context: Option<VideoPromptContext>,
        images: Vec<PromptImage>,
        saved_session: Option<SavedSessionHint>,
        client_settings: AcpClientSettings,
        profiles: AgentProfilesHint,
        on_event: F,
    ) -> Result<String, AcpError>
    where
        F: FnMut(AcpEvent),
    {
        self.execute(
            AgentExecutionRequest {
                text: text.as_ref().to_string(),
                cwd,
                profile_id,
                context,
                images,
                saved_session,
                client_settings,
                profiles,
                session_kind: SessionKind::Chat,
                attempt_label: None,
            },
            on_event,
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
        images: &[PromptImage],
        saved_session: Option<SavedSessionHint>,
        client_settings: AcpClientSettings,
        profiles: AgentProfilesHint,
        session_kind: SessionKind,
        on_event: F,
        attempt_label: Option<&str>,
    ) -> Result<String, AcpError>
    where
        F: FnMut(AcpEvent),
    {
        self.execute(
            AgentExecutionRequest {
                text: text.as_ref().to_string(),
                cwd,
                profile_id,
                context,
                images: images.to_vec(),
                saved_session,
                client_settings,
                profiles,
                session_kind,
                attempt_label: attempt_label.map(str::to_string),
            },
            on_event,
        )
    }

    /// Shared typed execution boundary for interactive chat and isolated
    /// domain tasks. Retry policy and durable task state live outside this
    /// method; this executes exactly one prompt in one session scope.
    pub(crate) fn execute<F>(
        &self,
        request: AgentExecutionRequest,
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
            // Best-effort pid peek: never block the failing path on the lock.
            let pid = self
                .session
                .try_lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|session| session.agent.id()));
            Self::log_workshop_exit(request.attempt_label.as_deref(), pid, "busy", None, 0);
            return Err(AcpError::busy());
        }
        self.cancel.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.permission_mode.lock() {
            *guard = request.client_settings.permission_mode;
        }

        let prepared = prepare_profiles(&request.profiles);
        let outcome = self.run_prompt_inner(
            request.text.as_str(),
            request.cwd.as_deref(),
            request.profile_id.as_deref(),
            request.context.as_ref(),
            &request.images,
            request.saved_session.as_ref(),
            &prepared,
            request.session_kind,
            &mut on_event,
            request.attempt_label.as_deref(),
        );

        // Graceful cancel keeps the connection: `session/cancel` aborts only
        // the current turn, the process and `sessionId` stay alive for the
        // next prompt. Only real failures drop the session. (The
        // cancel-timeout path already took + terminated the session inside
        // `run_prompt_inner`, so the drop below is a no-op there.)
        if should_drop_live_session(&outcome) {
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

    // Kept explicit so session lifecycle fields remain visible at the protocol boundary.
    #[allow(clippy::too_many_arguments)]
    fn run_prompt_inner(
        &self,
        prompt_text: &str,
        cwd: Option<&str>,
        profile_id: Option<&str>,
        context: Option<&VideoPromptContext>,
        images: &[PromptImage],
        saved_session: Option<&SavedSessionHint>,
        prepared: &PreparedProfiles,
        session_kind: SessionKind,
        on_event: &mut dyn FnMut(AcpEvent),
        attempt_label: Option<&str>,
    ) -> Result<(String, Option<String>), AcpError> {
        let prompt_text = prompt_text.trim();
        if let Err(error) = validate_prompt_images(images) {
            Self::log_workshop_exit(attempt_label, None, "bad-image", None, 0);
            return Err(error);
        }
        if prompt_text.is_empty() && images.is_empty() {
            Self::log_workshop_exit(attempt_label, None, "empty-prompt", None, 0);
            return Err(AcpError::bad_request("提问内容不能为空"));
        }

        let profile_override = profile_id;
        let resolved_profile = match resolve_active_profile(prepared, profile_override) {
            Ok(profile) => profile,
            Err(error) => {
                Self::log_workshop_exit(attempt_label, None, "profile-missing", None, 0);
                return Err(error);
            }
        };

        // Ensure live session (reuse only when it belongs to this profile —
        // a live Codex process must never answer prompts labeled Antigravity).
        {
            let mut guard = match self.session.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    Self::log_workshop_exit(attempt_label, None, "session-lock-failed", None, 0);
                    return Err(AcpError::internal(Some("ACP session mutex poisoned")));
                }
            };
            let reuse = guard
                .as_ref()
                .is_some_and(|session| session.profile_id == resolved_profile.id);
            if !reuse {
                if let Some(stale) = guard.take() {
                    tracing::info!(
                        stale_profile = %stale.profile_id,
                        target_profile = %resolved_profile.id,
                        "live agent belongs to another profile; winding down before spawn"
                    );
                    self.clear_cancel_writer();
                    self.wind_down_session(stale);
                }
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
                    session_kind,
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
                            Some(spawned.agent.id()),
                            "model-selection-failed",
                            None,
                            0,
                        );
                        return Err(error);
                    }
                }
                self.publish_cancel_writer(&spawned);
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
        // Copy: the EOF path takes the guard, so the hint must be owned here.
        let empty_reply_hint = session.empty_reply_hint.clone();

        if !images.is_empty() && !session.init.prompt_image {
            Self::log_workshop_exit(
                attempt_label,
                Some(session.agent.id()),
                "image-unsupported",
                None,
                0,
            );
            return Err(AcpError::bad_request("当前 Agent 不支持图片输入"));
        }

        if let Err(error) = resolve_session_cwd(cwd) {
            Self::log_workshop_exit(
                attempt_label,
                Some(session.agent.id()),
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
        let pid = session.agent.id();
        if let Err(error) = crate::runtime::io::write_request(
            &session.stdin,
            prompt_id,
            "session/prompt",
            crate::wire::session::session_prompt_params(
                &session.session_id,
                prompt_text,
                context,
                images,
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
                    &session.stdin,
                    "session/cancel",
                    session_cancel_params(&session.session_id),
                );
                cancel_sent = true;
                cancel_at = Some(Instant::now());
            }

            if Instant::now() > deadline {
                if let Some(mut taken) = guard.take() {
                    self.clear_cancel_writer();
                    taken.agent.terminate(false);
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
                        self.clear_cancel_writer();
                        taken.agent.terminate(false);
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
                    self.clear_cancel_writer();
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
                    Self::log_workshop_exit(
                        attempt_label,
                        Some(pid),
                        "eof",
                        None,
                        collector.chunk_count(),
                    );
                    return Err(AcpError::protocol(Some(
                        "ACP stdout closed while waiting for session/prompt",
                    )));
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
                        return self.empty_reply_outcome(&empty_reply_hint, stop);
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
    /// hint prose as JSON. The hint comes from the active profile. Unit-covered
    /// without a live agent process.
    fn empty_reply_outcome(
        &self,
        empty_reply_hint: &str,
        stop: Option<String>,
    ) -> Result<(String, Option<String>), AcpError> {
        if self.isolated_task() {
            return Err(AcpError::no_output(stop.as_deref()));
        }
        Ok((empty_reply_hint.to_string(), stop))
    }
}

/// Drop the live session on failure, except graceful cancel which keeps the
/// connection (process + `sessionId`) for the next turn.
fn should_drop_live_session(outcome: &Result<(String, Option<String>), AcpError>) -> bool {
    match outcome {
        Ok(_) => false,
        Err(error) => error.code != crate::AcpErrorCode::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_loop_bounds_are_locked() {
        // These bounds are the product contract for H-P2-7 (timeout + cancel).
        // Change them deliberately, never by accident.
        assert_eq!(crate::runtime::io::PROMPT_DEADLINE_SECS, 600);
        assert_eq!(crate::runtime::io::CANCEL_KILL_SECS, 8);
        assert!(crate::runtime::io::initialize_timeout() >= Duration::from_secs(60));
    }

    #[test]
    fn empty_reply_outcome_splits_chat_and_isolated() {
        use std::sync::atomic::Ordering;

        // Chat (tools enabled): keeps the human-readable hint as success.
        let chat = AcpService::new();
        let (text, stop) = chat
            .empty_reply_outcome(crate::agent::profile::CODEX_EMPTY_REPLY_HINT, None)
            .expect("chat keeps hint");
        assert!(text.contains("Codex"));
        assert_eq!(stop, None);

        let generic = AcpService::new();
        let (text, _) = generic
            .empty_reply_outcome(crate::agent::profile::GENERIC_EMPTY_REPLY_HINT, None)
            .expect("generic hint");
        assert!(!text.contains("Codex"));

        // Isolated task: typed error carrying the stop reason, never prose.
        let isolated = AcpService::new();
        isolated.tool_access_enabled.store(false, Ordering::SeqCst);
        let err = isolated
            .empty_reply_outcome(
                crate::agent::profile::CODEX_EMPTY_REPLY_HINT,
                Some("end_turn".into()),
            )
            .expect_err("isolated errors");
        assert_eq!(err.code, crate::AcpErrorCode::NoOutput);
        assert_eq!(err.details.as_deref(), Some("end_turn"));
    }

    #[test]
    fn graceful_cancel_keeps_session_while_failures_drop() {
        assert!(!should_drop_live_session(&Ok((
            "text".into(),
            Some("end_turn".into())
        ))));
        assert!(!should_drop_live_session(&Err(AcpError::cancelled())));
        assert!(should_drop_live_session(&Err(AcpError::protocol(Some(
            "boom"
        )))));
        assert!(should_drop_live_session(&Err(AcpError::busy())));
    }
}
