//! AcpService — optional on-demand ACP Client over stdio JSON-RPC.
//!
//! Baseline Client→Agent: initialize, authenticate, session/new|prompt|cancel|close.
//! Agent→Client: session/update, session/request_permission, fs/*, terminal/*.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::acp::context::{self, VideoPromptContext};
use crate::acp::error::AcpError;
use crate::acp::host::AcpHost;
use crate::acp::model::{
    AcpEvent, AcpModelDiscoveryResult, AcpSessionModelOptions, AcpSessionModelSelection, AcpStatus,
    AgentProfilesHint, PermissionOption, SavedSessionHint,
};
use crate::acp::paths::{resolve_session_cwd, status_from_profiles};
use crate::mcp::{
    lumina_mcp_servers, snapshot_path_for_cwd, write_snapshot, LuminaMcpSnapshot,
    PromptSnapshotState,
};
use crate::acp::profile::{
    prepare_profiles, resolve_active_profile, resolve_launch, AgentKind, PreparedProfiles,
};
use crate::acp::protocol::{
    authenticate_params, classify_inbound, encode_line, error_response, extract_agent_text,
    extract_permission_options, extract_plan_summary, extract_thought_text, extract_tool_call,
    extract_tool_call_content_chunk,
    initialize_params, initialize_params_restricted, is_error_response, notification,
    parse_initialize_result, parse_session_id, parse_session_model_options, parse_stop_reason,
    permission_auto_result, permission_cancelled_result, permission_selected_result,
    pick_auth_method, request, session_cancel_params, session_close_params, session_new_params,
    session_resume_params, session_set_config_option_params, success_response, Inbound,
    InitializeResult,
};
use crate::acp::settings::{AcpClientSettings, PermissionMode};
use crate::library::MediaLibraryService;

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
    prompt_snapshot_state: Mutex<PromptSnapshotState>,
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
            prompt_snapshot_state: Mutex::new(PromptSnapshotState::default()),
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
            status.session_model_options = guard.as_ref().map(|session| session.model_options.clone());
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
    pub fn close_session(&self) -> Result<(), AcpError> {
        self.cancel.store(true, Ordering::SeqCst);
        self.drop_live_session();
        self.reset_prompt_snapshot_state();
        self.cancel.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn write_prompt_snapshot(
        &self,
        cwd: &std::path::Path,
        snapshot: &LuminaMcpSnapshot,
    ) -> Result<std::path::PathBuf, AcpError> {
        let path = snapshot_path_for_cwd(cwd);
        write_snapshot(&path, snapshot)
            .map_err(|details| AcpError::internal(Some(&details)))?;
        Ok(path)
    }

    pub fn build_prompt_snapshot(
        &self,
        context: Option<&VideoPromptContext>,
        library: &MediaLibraryService,
        vision_capable: bool,
    ) -> Result<LuminaMcpSnapshot, AcpError> {
        let mut guard = self
            .prompt_snapshot_state
            .lock()
            .map_err(|_| AcpError::internal(Some("prompt snapshot mutex poisoned")))?;
        guard
            .next_snapshot(context, library, vision_capable)
            .map_err(|error| AcpError::internal(error.details.as_deref()))
    }

    pub fn reset_prompt_snapshot_state(&self) {
        if let Ok(mut guard) = self.prompt_snapshot_state.lock() {
            guard.reset();
        }
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
            if let Some(session) = guard.as_mut() {
                if let Some(selection) = client_settings.model_selection() {
                    let _ =
                        self.apply_model_selection(session, &selection, &mut on_event);
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
                self.drop_live_session();
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
        self.reset_prompt_snapshot_state();
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
                if let Err(error) = Self::close_agent_session(
                    self,
                    session,
                    &mut on_event,
                ) {
                    tracing::warn!(%error, "session/close failed during new chat; respawning agent");
                    drop(guard);
                    self.drop_live_session();
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

            let new_session_id =
                self.create_new_session(session, &cwd_string, &profile.id, &mut on_event)?;
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
            self.connect(
                cwd,
                profile_id,
                None,
                client_settings,
                profiles,
                on_event,
            )
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
            return Err(AcpError::protocol(Some(&format!("session/close: {message}"))));
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
        let session = guard.as_mut().ok_or_else(|| {
            AcpError::protocol(Some("no active agent session"))
        })?;

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

    fn drop_live_session(&self) {
        if let Ok(mut guard) = self.session.lock() {
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
                }
                let _ = session.child.kill();
                let _ = session.child.wait();
            }
        }
        self.host.release_all();
    }

    // This public boundary mirrors the explicit ACP/Tauri request fields.
    #[allow(clippy::too_many_arguments)]
    pub fn prompt<F>(
        &self,
        text: impl AsRef<str>,
        cwd: Option<String>,
        profile_id: Option<String>,
        context: Option<VideoPromptContext>,
        saved_session: Option<SavedSessionHint>,
        client_settings: AcpClientSettings,
        profiles: AgentProfilesHint,
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
        if let Ok(mut guard) = self.permission_mode.lock() {
            *guard = client_settings.permission_mode;
        }

        let prepared = prepare_profiles(&profiles);
        let outcome = self.run_prompt_inner(
            text.as_ref(),
            cwd.as_deref(),
            profile_id.as_deref(),
            context.as_ref(),
            saved_session.as_ref(),
            &prepared,
            &mut on_event,
        );

        if outcome.is_err() {
            self.drop_live_session();
        }

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

    /// Run one prompt in a fresh ACP process/session using an existing profile,
    /// then close it. This is deliberately separate from the interactive chat
    /// service: no saved session, no chat context, and no Agent tool access.
    pub fn prompt_isolated_restricted(
        text: impl AsRef<str>,
        profile_id: String,
        profiles: AgentProfilesHint,
        model_selection: Option<AcpSessionModelSelection>,
    ) -> Result<String, AcpError> {
        let service = Self::new();
        service.tool_access_enabled.store(false, Ordering::SeqCst);
        if let Ok(mut selection) = service.next_session_model_selection.lock() {
            *selection = model_selection;
        }
        let outcome = service.prompt(
            text,
            None,
            Some(profile_id),
            None,
            None,
            AcpClientSettings {
                permission_mode: PermissionMode::Ask,
                thinking_level: crate::acp::settings::ThinkingLevel::Hidden,
                agent_mode: "metadata-resolver".into(),
                vision_capable: false,
                model_id: None,
                reasoning_effort: None,
            },
            profiles,
            |_| {},
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
            service.spawn_session(None, None, &prepared, Some(&profile_id), &mut |_| {})?;
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
        saved_session: Option<&SavedSessionHint>,
        prepared: &PreparedProfiles,
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<(String, Option<String>), AcpError> {
        let prompt_text = prompt_text.trim();
        if prompt_text.is_empty() {
            return Err(AcpError::bad_request("提问内容不能为空"));
        }

        let profile_override = profile_id;

        // Ensure live session (reuse when possible).
        {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
            if guard.is_none() {
                let mut spawned =
                    self.spawn_session(cwd, saved_session, prepared, profile_override, on_event)?;
                let selection = self
                    .next_session_model_selection
                    .lock()
                    .ok()
                    .and_then(|mut selection| selection.take());
                if let Some(selection) = selection {
                    self.apply_model_selection(&mut spawned, &selection, on_event)?;
                }
                *guard = Some(spawned);
            }
        }

        let mut guard = self
            .session
            .lock()
            .map_err(|_| AcpError::internal(Some("ACP session mutex poisoned")))?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AcpError::internal(Some("ACP session missing after spawn")))?;

        let _ = resolve_session_cwd(cwd)?;

        on_event(AcpEvent::Progress {
            message: "正在发送问题…".into(),
        });

        let prompt_id = session.next_id;
        session.next_id += 1;
        Self::write_request(
            &mut session.stdin,
            prompt_id,
            "session/prompt",
            context::session_prompt_params(&session.session_id, prompt_text, context),
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
                self,
                session,
                Duration::from_millis(250),
                &self.cancel,
                &self.host,
                &mut on_event_collect,
            )? {
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
                    if stop.as_deref() == Some("cancelled") || self.cancel.load(Ordering::SeqCst) {
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
        cwd_hint: Option<&str>,
        saved_session: Option<&SavedSessionHint>,
        prepared: &PreparedProfiles,
        profile_id: Option<&str>,
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

        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .current_dir(&workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &launch.env {
            command.env(key, value);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(|error| {
            tracing::warn!(%error, cwd = %cwd, "ACP spawn failed");
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
                let _ = session.child.kill();
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

        on_event(AcpEvent::Progress {
            message: "正在创建会话…".into(),
        });

        let saved = saved_session.cloned();
        let try_resume = supports_resume
            && saved.as_ref().is_some_and(|s| {
                s.profile_id == profile.id && s.cwd == cwd && !s.session_id.is_empty()
            });

        let session_id = if try_resume {
            let saved = saved.expect("checked above");
            on_event(AcpEvent::Progress {
                message: "正在恢复上次会话…".into(),
            });
            let resume_id = session.next_id;
            session.next_id += 1;
            Self::write_request(
                &mut session.stdin,
                resume_id,
                "session/resume",
                session_resume_params(
                    &saved.session_id,
                    &cwd,
                    lumina_mcp_servers(&snapshot_path_for_cwd(std::path::Path::new(&cwd))),
                ),
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
                    self.create_new_session(&mut session, &cwd, &profile.id, on_event)?
                }
            }
        } else {
            self.create_new_session(&mut session, &cwd, &profile.id, on_event)?
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
        on_event: &mut dyn FnMut(AcpEvent),
    ) -> Result<String, AcpError> {
        let snapshot_path = snapshot_path_for_cwd(std::path::Path::new(cwd));
        let _ = write_snapshot(
            &snapshot_path,
            &LuminaMcpSnapshot::empty(),
        );
        let new_id = session.next_id;
        session.next_id += 1;
        let mcp_servers = lumina_mcp_servers(&snapshot_path_for_cwd(std::path::Path::new(cwd)));
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
        let session_id = parse_session_id(&session_resp).ok_or_else(|| {
            let _ = session.child.kill();
            AcpError::protocol(Some(&format!("missing sessionId: {session_resp}")))
        })?;
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
                    tracing::debug!(line = trimmed, %error, "ACP skipping non-JSON stdout line");
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
