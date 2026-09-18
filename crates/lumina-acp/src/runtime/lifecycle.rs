//! Live ACP session lifecycle: spawn, session/new|resume|close, rotation.
//!
//! Pure move from `runtime/service.rs` (no behavior change).

use std::io::{BufRead, BufReader};
use std::process::{ChildStdin, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::agent::launch::{pick_auth_method, resolve_launch};
use crate::agent::profile::{resolve_active_profile, PreparedProfiles};
use crate::agent::workspace::{resolve_session_cwd, same_workspace};
use crate::domain::environment::session_env;
use crate::domain::model::{
    AcpEvent, AcpSessionModelOptions, AcpSessionModelSelection, AgentSessionListResult,
    ResumeOutcome, SavedSessionHint, SessionKind,
};
use crate::error::AcpError;
use crate::runtime::host::AcpHost;
use crate::runtime::io::{initialize_timeout, read_until_id_raw, write_request};
use crate::runtime::process::{command, AgentProcess};
use crate::runtime::service::AcpService;
use crate::wire::codec::is_error_response;
use crate::wire::session::{
    authenticate_params, classify_resume_failure, initialize_params, initialize_params_restricted,
    parse_initialize_result, parse_session_id, parse_session_list, parse_session_model_options,
    session_close_params, session_list_params, session_new_params, session_resume_params,
    InitializeResult,
};

/// How many `session/list` pages one history query may walk.
///
/// The agent paginates over all of its history by timestamp and filters by
/// `cwd` per page, so it keeps handing out a cursor even when a page matched
/// nothing. Every extra page is another round trip plus disk work on the agent
/// side, so this caps the cost; hitting the cap only means we cannot claim a
/// stored conversation is gone (see `AgentSessionListTrust`).
const PAGE_BUDGET: usize = 5;

/// How long a session close may hold up switching media or starting a new chat.
const CLOSE_FLUSH_BUDGET: Duration = Duration::from_secs(3);

/// Everything that describes the session about to be created. Grouped because
/// the argument list had grown past the point where call sites were readable.
pub(crate) struct NewSessionSpec<'a> {
    pub(crate) cwd: &'a str,
    pub(crate) profile_id: &'a str,
    pub(crate) vision_capable: bool,
    pub(crate) kind: SessionKind,
    /// Why we create instead of restore, so the UI can tell an occupied
    /// conversation from a lost one. `None` when no restore was attempted.
    pub(crate) resume: Option<ResumeOutcome>,
}

/// stdin has its own lock so `session/cancel` can be written while the
/// prompt loop is blocked reading stdout. The prompt loop keeps holding the
/// session mutex across `read_line`; cancel only clones this handle under a
/// brief session lock and writes outside of it.
pub(crate) type SharedStdin = Arc<Mutex<ChildStdin>>;

pub(crate) struct LiveSession {
    pub(crate) agent: AgentProcess,
    pub(crate) stdin: SharedStdin,
    pub(crate) reader: BufReader<std::process::ChildStdout>,
    pub(crate) session_id: String,
    pub(crate) next_id: u64,
    pub(crate) init: InitializeResult,
    /// Profile-derived empty-reply copy; the prompt loop owns it because the
    /// EOF path takes the session guard.
    pub(crate) empty_reply_hint: String,
    pub(crate) model_options: AcpSessionModelOptions,
}

/// P1: a LiveSession always owns a live agent tree. Any path that drops one
/// without an explicit close (setup failure, abandoned take, early return)
/// terminates the whole tree instead of leaking it. Never blocks (`false`):
/// explicit close paths terminate deterministically first, and a repeated
/// termination of the dead tree is a no-op.
impl Drop for LiveSession {
    fn drop(&mut self) {
        self.agent.terminate(false);
    }
}

impl AcpService {
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
    #[cfg(test)]
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
        let new_id = match self.create_new_session(
            &mut session,
            NewSessionSpec {
                cwd: &cwd,
                profile_id,
                vision_capable: false,
                kind: SessionKind::Workshop,
                resume: None,
            },
            on_event,
        ) {
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

    pub(crate) fn close_agent_session(
        service: &Self,
        session: &mut LiveSession,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(), AcpError> {
        let request_id = session.next_id;
        session.next_id += 1;
        write_request(
            &session.stdin,
            request_id,
            "session/close",
            session_close_params(&session.session_id),
        )?;
        let response = read_until_id_raw(
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

    /// Validate saved conversation ids against the already-live Agent.
    ///
    /// This path is deliberately read-only and never calls `spawn_session`:
    /// opening the history panel must not start an Agent process or session.
    pub fn list_agent_sessions(
        &self,
        cwd: Option<&str>,
    ) -> Result<AgentSessionListResult, AcpError> {
        if self.is_busy() {
            return Ok(AgentSessionListResult::default());
        }

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        let Some(session) = guard.as_mut() else {
            return Ok(AgentSessionListResult::default());
        };
        if !session.init.supports_session_list {
            return Ok(AgentSessionListResult::default());
        }

        // Timed per page, not just in total: without the split, a slow history
        // panel is indistinguishable from a slow agent startup in the log.
        let started = Instant::now();
        let mut cursor: Option<String> = None;
        let mut sessions = Vec::new();
        let mut truncated = false;
        let mut pages = 0usize;
        for page in 0..PAGE_BUDGET {
            let request_id = session.next_id;
            session.next_id += 1;
            write_request(
                &session.stdin,
                request_id,
                "session/list",
                session_list_params(cwd, cursor.as_deref()),
            )?;
            let page_started = Instant::now();
            let response = read_until_id_raw(
                self,
                session,
                request_id,
                Duration::from_secs(30),
                &self.cancel,
                &self.host,
                &mut |_| {},
            )?;
            if let Some(message) = is_error_response(&response) {
                return Err(AcpError::protocol(Some(&format!(
                    "session/list: {message}"
                ))));
            }
            let (mut page_sessions, next_cursor) = parse_session_list(&response);
            pages += 1;
            tracing::info!(
                page,
                matched = page_sessions.len(),
                elapsed_ms = page_started.elapsed().as_millis(),
                "ACP session/list page"
            );
            sessions.append(&mut page_sessions);
            let Some(next_cursor) = next_cursor.filter(|value| !value.trim().is_empty()) else {
                break;
            };
            if page + 1 == PAGE_BUDGET {
                truncated = true;
                break;
            }
            cursor = Some(next_cursor);
        }

        tracing::info!(
            pages,
            matched = sessions.len(),
            truncated,
            elapsed_ms = started.elapsed().as_millis(),
            cwd = cwd.unwrap_or("<all>"),
            "ACP session/list done"
        );

        Ok(AgentSessionListResult {
            verified: true,
            sessions,
            truncated,
        })
    }

    pub(crate) fn drop_live_session(&self, wait_for_child: bool) {
        if let Ok(mut guard) = self.session.lock() {
            if let Some(mut session) = guard.take() {
                self.clear_cancel_writer();
                if wait_for_child {
                    self.wind_down_session(session);
                } else {
                    // App exit: no grace period, the tree goes now. Killing the
                    // whole job matters here (a wrapper-only kill orphaned the
                    // second Codex process and bricked the conversation).
                    session.agent.terminate(false);
                }
            }
        }
        if wait_for_child {
            self.host.release_all();
        } else {
            self.host.release_all_for_shutdown();
        }
    }

    /// Let the Agent persist the conversation, then kill the subtree off-thread.
    ///
    /// Sending `session/close` and killing right after is not enough: Codex
    /// writes the thread's rollout while winding the conversation down, and the
    /// job object takes out `codex.exe` mid-flush, so the conversation is lost
    /// and every later resume answers `no rollout found for thread id`.
    ///
    /// The wind-down cannot be awaited here. `read_one` blocks in `read_line`,
    /// so waiting for the close response would hang media switching for as
    /// long as the agent stays quiet. A fixed grace period on a detached
    /// thread keeps the caller instant and still bounds the agent's lifetime,
    /// the same trade already made for `session/cancel`.
    fn wind_down_session(&self, mut session: LiveSession) {
        if !session.init.supports_session_close {
            session.agent.terminate(false);
            return;
        }

        let id = session.next_id;
        session.next_id += 1;
        let params = session_close_params(&session.session_id);
        if write_request(&session.stdin, id, "session/close", params).is_err() {
            session.agent.terminate(false);
            return;
        }

        let session_id = session.session_id.clone();
        if let Err(error) = thread::Builder::new()
            .name("acp-wind-down".into())
            .spawn(move || {
                thread::sleep(CLOSE_FLUSH_BUDGET);
                session.agent.terminate(true);
                tracing::info!(session_id, "Agent subtree terminated after close grace");
            })
        {
            // The closure owned the session, so dropping it here already
            // terminated the tree via `Drop for LiveSession`.
            tracing::warn!(%error, "could not detach Agent wind-down; subtree dropped");
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_session(
        &self,
        cwd_hint: Option<&str>,
        saved_session: Option<&SavedSessionHint>,
        prepared: &PreparedProfiles,
        profile_id: Option<&str>,
        vision_capable: bool,
        session_kind: SessionKind,
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

        let mut command = command(&launch.program);
        command
            .args(&launch.args)
            .current_dir(&workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Strip PyInstaller environment variables explicitly from inherited parent env
        for (key, _) in std::env::vars() {
            if key.starts_with("_PYI") || key.starts_with("_MEI") {
                command.env_remove(&key);
            }
        }

        for (key, value) in &launch.env {
            command.env(key, value);
        }

        tracing::info!(
            profile_id = %profile.id,
            profile_kind = ?profile.kind,
            launcher = ?profile.launcher,
            env_preset = ?profile.env_preset,
            auth_policy = ?profile.auth_policy,
            program = %launch.program.display(),
            argument_count = launch.args.len(),
            cwd = %workspace.display(),
            "ACP launch prepared"
        );
        let child = command.spawn().map_err(|error| {
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

        // Take ownership of the whole subtree before the adapter has time to
        // spawn its Codex layers, so no descendant can outlive this session.
        let mut agent = AgentProcess::adopt(child);

        // Drain stderr so the pipe never blocks the agent, and detect OAuth URLs.
        if let Some(stderr) = agent.stderr() {
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
                        // Detect Google OAuth authentication link
                        if line.contains("https://accounts.google.com/o/oauth2/") {
                            if let Some(start) = line.find("https://accounts.google.com/") {
                                let url = line[start..]
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or(&line[start..]);
                                tracing::info!(
                                    oauth_url = url,
                                    "Detected Google OAuth URL, opening browser"
                                );
                                #[cfg(windows)]
                                {
                                    let _ = std::process::Command::new("rundll32")
                                        .args(["url.dll,FileProtocolHandler", url])
                                        .spawn();
                                }
                                #[cfg(target_os = "macos")]
                                {
                                    let _ = std::process::Command::new("open").arg(url).spawn();
                                }
                                #[cfg(target_os = "linux")]
                                {
                                    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
                                }
                            }
                        }
                    }
                    buf.clear();
                }
            });
        }

        let stdin = match agent.stdin() {
            Some(stdin) => stdin,
            None => {
                agent.terminate(false);
                return Err(AcpError::spawn_failed(Some("stdin pipe missing")));
            }
        };
        let stdout = match agent.stdout() {
            Some(stdout) => stdout,
            None => {
                agent.terminate(false);
                return Err(AcpError::spawn_failed(Some("stdout pipe missing")));
            }
        };
        let reader = BufReader::new(stdout);

        let mut session = LiveSession {
            agent,
            stdin: Arc::new(Mutex::new(stdin)),
            reader,
            session_id: String::new(),
            next_id: 1,
            init: InitializeResult::default(),
            empty_reply_hint: profile.empty_reply_hint(),
            model_options: AcpSessionModelOptions::default(),
        };

        on_event(AcpEvent::Progress {
            message: "正在初始化会话…".into(),
        });
        let init_id = session.next_id;
        session.next_id += 1;
        write_request(
            &session.stdin,
            init_id,
            "initialize",
            if self.tool_access_enabled.load(Ordering::SeqCst) {
                initialize_params()
            } else {
                initialize_params_restricted()
            },
        )?;
        // Agent cold start lands here, so it must be separable from whatever
        // session work follows when reading a slow connect in the log.
        let init_started = Instant::now();
        let init_resp = read_until_id_raw(
            self,
            &mut session,
            init_id,
            initialize_timeout(),
            &self.cancel,
            &self.host,
            on_event,
        )?;
        tracing::info!(
            elapsed_ms = init_started.elapsed().as_millis(),
            "ACP initialize completed"
        );
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
        if let Some(method) = pick_auth_method(&profile, &init) {
            on_event(AcpEvent::Progress {
                message: format!("正在认证（{}）…", method.name),
            });
            let auth_id = session.next_id;
            session.next_id += 1;
            write_request(
                &session.stdin,
                auth_id,
                "authenticate",
                authenticate_params(&method.id),
            )?;
            let auth_resp = read_until_id_raw(
                self,
                &mut session,
                auth_id,
                Duration::from_secs(120),
                &self.cancel,
                &self.host,
                on_event,
            )?;
            if let Some(msg) = is_error_response(&auth_resp) {
                session.agent.terminate(false);
                return Err(
                    profile.auth_failure_error(Some(&format!("authenticate failed: {msg}")))
                );
            }
        }

        session.init = init.clone();
        let session_id = self.open_session_on_live_process(
            &mut session,
            saved_session,
            NewSessionSpec {
                cwd: &cwd,
                profile_id: &profile.id,
                vision_capable,
                kind: session_kind,
                resume: None,
            },
            on_event,
        )?;

        session.session_id = session_id;
        workspace_guard.1 = false;
        Ok(session)
    }

    /// Open (resume when possible, else create) a session on an ALREADY-LIVE
    /// agent process. Shared by fresh spawn and same-process rotation
    /// (`switch_session`), so both paths resume-or-create identically.
    pub(crate) fn open_session_on_live_process(
        &self,
        session: &mut LiveSession,
        saved_session: Option<&SavedSessionHint>,
        spec: NewSessionSpec<'_>,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<String, AcpError> {
        let NewSessionSpec {
            cwd,
            profile_id,
            vision_capable,
            kind,
            resume: _,
        } = spec;
        let supports_resume = session.init.supports_session_resume;

        // Snapshot IO goes through the app-provided environment (M5).
        let env = session_env()?;
        let snapshot_path = env.snapshot_path(std::path::Path::new(cwd));
        env.sync_snapshot(&snapshot_path, vision_capable)
            .map_err(|error| AcpError::internal(Some(&error)))?;
        let isolated = self.isolated_task();
        let mcp_servers = env.mcp_servers(&snapshot_path, isolated);
        tracing::info!(
            session_kind = ?kind,
            isolated,
            cwd,
            snapshot = %snapshot_path.display(),
            mcp = %mcp_servers,
            "prepared Lumina MCP for ACP session"
        );

        on_event(AcpEvent::Progress {
            message: "正在创建会话…".into(),
        });

        let saved = saved_session.cloned();
        // Workspace compare must ignore spelling: the request `cwd` is
        // canonicalized (`\\?\D:\...` verbatim on Windows) while a stored
        // hint keeps the frontend spelling (`D:\...`). A plain `==` reports
        // `scope_mismatch` forever and resume is never attempted.
        let try_resume = supports_resume
            && saved.as_ref().is_some_and(|s| {
                s.profile_id == profile_id
                    && same_workspace(&s.cwd, cwd)
                    && !s.session_id.is_empty()
            });

        if try_resume {
            let Some(saved) = saved else {
                return Err(AcpError::internal(Some("saved session missing for resume")));
            };
            let hint_short: String = saved.session_id.chars().take(8).collect();
            tracing::info!(
                hint = %hint_short,
                session_kind = ?kind,
                "session/resume attempting"
            );
            on_event(AcpEvent::Progress {
                message: "正在恢复上次会话…".into(),
            });
            let resume_id = session.next_id;
            session.next_id += 1;
            write_request(
                &session.stdin,
                resume_id,
                "session/resume",
                session_resume_params(&saved.session_id, cwd, mcp_servers),
            )?;
            let resume_started = Instant::now();
            let resume_resp = read_until_id_raw(
                self,
                session,
                resume_id,
                Duration::from_secs(60),
                &self.cancel,
                &self.host,
                on_event,
            );
            match resume_resp {
                Ok(response) => {
                    session.model_options = parse_session_model_options(&response);
                    tracing::info!(
                        elapsed_ms = resume_started.elapsed().as_millis(),
                        session_kind = ?kind,
                        session_id = %saved.session_id,
                        "session/resume succeeded"
                    );
                    on_event(AcpEvent::Progress {
                        message: "已恢复上次会话".into(),
                    });
                    on_event(AcpEvent::SessionSaved {
                        session_id: saved.session_id.clone(),
                        profile_id: profile_id.to_string(),
                        cwd: cwd.to_string(),
                        resume: Some(ResumeOutcome::Resumed),
                    });
                    Ok(saved.session_id)
                }
                Err(error) => {
                    let outcome = classify_resume_failure(error.details.as_deref());
                    tracing::warn!(
                        %error,
                        ?outcome,
                        elapsed_ms = resume_started.elapsed().as_millis(),
                        hint = %hint_short,
                        "session/resume failed; creating new session"
                    );
                    self.create_new_session(
                        session,
                        NewSessionSpec {
                            cwd,
                            profile_id,
                            vision_capable,
                            kind,
                            resume: Some(outcome),
                        },
                        on_event,
                    )
                }
            }
        } else {
            let resume_skip_reason = if !supports_resume {
                "unsupported"
            } else {
                match saved.as_ref() {
                    Some(hint) if hint.session_id.is_empty() => "no_hint",
                    Some(hint)
                        if hint.profile_id != profile_id || !same_workspace(&hint.cwd, cwd) =>
                    {
                        "scope_mismatch"
                    }
                    Some(_) => "scope_mismatch",
                    None => "no_hint",
                }
            };
            let skip_hint_short: String = match saved.as_ref() {
                Some(hint) => hint.session_id.chars().take(8).collect(),
                None => String::new(),
            };
            tracing::info!(
                reason = resume_skip_reason,
                hint = %skip_hint_short,
                session_kind = ?kind,
                // Both sides of the scope check: a future `scope_mismatch`
                // must be diagnosable without guessing which field drifted.
                want_profile = profile_id,
                want_cwd = cwd,
                got_profile = saved.as_ref().map(|s| s.profile_id.as_str()).unwrap_or(""),
                got_cwd = saved.as_ref().map(|s| s.cwd.as_str()).unwrap_or(""),
                "session/resume skipped; creating new session"
            );
            self.create_new_session(
                session,
                NewSessionSpec {
                    cwd,
                    profile_id,
                    vision_capable,
                    kind,
                    resume: None,
                },
                on_event,
            )
        }
    }

    pub(crate) fn create_new_session(
        &self,
        session: &mut LiveSession,
        spec: NewSessionSpec<'_>,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<String, AcpError> {
        let NewSessionSpec {
            cwd,
            profile_id,
            vision_capable,
            kind,
            resume,
        } = spec;
        // Snapshot IO goes through the app-provided environment (M5).
        let env = session_env()?;
        let snapshot_path = env.snapshot_path(std::path::Path::new(cwd));
        env.sync_snapshot(&snapshot_path, vision_capable)
            .map_err(|error| AcpError::internal(Some(&error)))?;
        let new_id = session.next_id;
        session.next_id += 1;
        let isolated = self.isolated_task();
        let mcp_servers = env.mcp_servers(&snapshot_path, isolated);
        tracing::info!(
            session_kind = ?kind,
            isolated,
            cwd,
            snapshot = %snapshot_path.display(),
            mcp = %mcp_servers,
            "registering Lumina MCP for ACP session"
        );
        write_request(
            &session.stdin,
            new_id,
            "session/new",
            session_new_params(cwd, mcp_servers, kind),
        )?;
        // Covers the agent bringing the Lumina MCP server up and running
        // `tools/list` against it, which is the slow part of a cold session.
        let new_started = Instant::now();
        let session_resp = read_until_id_raw(
            self,
            session,
            new_id,
            Duration::from_secs(60),
            &self.cancel,
            &self.host,
            on_event,
        )?;
        let Some(session_id) = parse_session_id(&session_resp) else {
            session.agent.terminate(false);
            return Err(AcpError::protocol(Some(&format!(
                "missing sessionId: {session_resp}"
            ))));
        };
        tracing::info!(
            elapsed_ms = new_started.elapsed().as_millis(),
            session_kind = ?kind,
            session_id = %session_id,
            "session/new completed"
        );
        session.model_options = parse_session_model_options(&session_resp);
        on_event(AcpEvent::SessionSaved {
            session_id: session_id.clone(),
            profile_id: profile_id.to_string(),
            cwd: cwd.to_string(),
            resume,
        });
        Ok(session_id)
    }
}
