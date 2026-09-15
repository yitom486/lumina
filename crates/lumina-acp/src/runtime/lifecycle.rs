//! Live ACP session lifecycle: spawn, session/new|resume|close, rotation.
//!
//! Pure move from `runtime/service.rs` (no behavior change).

use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use crate::agent::launch::resolve_launch;
use crate::agent::profile::{resolve_active_profile, AgentKind, PreparedProfiles};
use crate::agent::workspace::resolve_session_cwd;
use crate::domain::environment::session_env;
use crate::domain::model::{
    AcpEvent, AcpSessionModelOptions, AcpSessionModelSelection, SavedSessionHint,
};
use crate::error::AcpError;
use crate::runtime::host::AcpHost;
use crate::runtime::io::{initialize_timeout, read_until_id_raw, write_request};
use crate::runtime::process::{command, terminate_tree};
use crate::runtime::service::AcpService;
use crate::wire::codec::is_error_response;
use crate::wire::session::{
    authenticate_params, initialize_params, initialize_params_restricted, parse_initialize_result,
    parse_session_id, parse_session_model_options, pick_auth_method, session_close_params,
    session_new_params, session_resume_params, InitializeResult,
};

pub(crate) struct LiveSession {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) reader: BufReader<std::process::ChildStdout>,
    pub(crate) session_id: String,
    pub(crate) next_id: u64,
    pub(crate) init: InitializeResult,
    pub(crate) profile_kind: AgentKind,
    pub(crate) model_options: AcpSessionModelOptions,
}

/// P1: a LiveSession always owns a live agent tree. Any path that drops one
/// without an explicit close (setup failure, abandoned take, early return)
/// terminates the whole tree instead of leaking it. Never blocks (`false`):
/// explicit close paths terminate deterministically first, and a repeated
/// taskkill against the dead tree fails silently.
impl Drop for LiveSession {
    fn drop(&mut self) {
        terminate_tree(&mut self.child, false);
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

    pub(crate) fn close_agent_session(
        service: &Self,
        session: &mut LiveSession,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(), AcpError> {
        let request_id = session.next_id;
        session.next_id += 1;
        write_request(
            &mut session.stdin,
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

    pub(crate) fn drop_live_session(&self, wait_for_child: bool) {
        if let Ok(mut guard) = self.session.lock() {
            if let Some(mut session) = guard.take() {
                if wait_for_child && session.init.supports_session_close {
                    let id = session.next_id;
                    session.next_id += 1;
                    let _ = write_request(
                        &mut session.stdin,
                        id,
                        "session/close",
                        session_close_params(&session.session_id),
                    );
                }
                // P1: kill the whole tree (wrapper-only kill orphaned the
                // second Codex process). Graceful close above stays first.
                terminate_tree(&mut session.child, wait_for_child);
            }
        }
        if wait_for_child {
            self.host.release_all();
        } else {
            self.host.release_all_for_shutdown();
        }
    }

    pub(crate) fn spawn_session(
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

        let mut command = command(&launch.program);
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
                terminate_tree(&mut child, false);
                return Err(AcpError::spawn_failed(Some("stdin pipe missing")));
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                terminate_tree(&mut child, false);
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
        write_request(
            &mut session.stdin,
            init_id,
            "initialize",
            if self.tool_access_enabled.load(Ordering::SeqCst) {
                initialize_params()
            } else {
                initialize_params_restricted()
            },
        )?;
        let init_resp = read_until_id_raw(
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
            write_request(
                &mut session.stdin,
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
                terminate_tree(&mut session.child, false);
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
            write_request(
                &mut session.stdin,
                resume_id,
                "session/resume",
                session_resume_params(&saved.session_id, &cwd, mcp_servers),
            )?;
            let resume_resp = read_until_id_raw(
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

    pub(crate) fn create_new_session(
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
        write_request(
            &mut session.stdin,
            new_id,
            "session/new",
            session_new_params(cwd, mcp_servers),
        )?;
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
            terminate_tree(&mut session.child, false);
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
}
