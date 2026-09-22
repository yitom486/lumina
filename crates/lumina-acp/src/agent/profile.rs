//! Pluggable ACP Agent profiles (default: Codex adapter → App Server).
//! Profiles are persisted in the frontend (Zustand); Rust only receives hints per invoke.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use crate::domain::model::{
    AgentKind, AgentProfileStatus, AuthPolicy, EnvPreset, LauncherPreset, SessionStoragePreset,
};
pub use crate::domain::model::{AgentProfileInput, AgentProfilesHint};
use crate::error::{AcpError, AcpErrorCode};

use super::launch::{resolve_launch, CODEX_ACP_PACKAGE};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    /// Executable name or absolute path (e.g. `codex-acp`, `C:\\tools\\codex-acp.exe`).
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub launcher: Option<LauncherPreset>,
    pub env_preset: Option<EnvPreset>,
    pub auth_policy: Option<AuthPolicy>,
    #[serde(default)]
    pub auth_methods: Vec<String>,
    pub session_storage: Option<SessionStoragePreset>,
}

pub(crate) const CODEX_EMPTY_REPLY_HINT: &str =
    "（会话结束，未解析到文本回复；请确认 Codex 已登录，且模型走 Responses API）";
pub(crate) const ANTIGRAVITY_EMPTY_REPLY_HINT: &str =
    "（会话结束，未解析到文本回复；请确认 Google 账号已授权且网络代理正常）";
pub(crate) const GENERIC_EMPTY_REPLY_HINT: &str =
    "（会话结束，未解析到文本回复；请确认该 ACP Agent 可用）";
const CODEX_AUTH_ERROR_MESSAGE: &str =
    "Codex 尚未登录或 API 未配置，请在终端运行 codex login 后重试";
const ANTIGRAVITY_AUTH_ERROR_MESSAGE: &str =
    "Google 账号尚未授权，请点击「登录 Google 账号」完成授权并在代理通畅下重试";
const GENERIC_AUTH_ERROR_MESSAGE: &str =
    "该 AI Agent 尚未完成登录或认证，请按其官方指引完成登录后重试";
const ANTIGRAVITY_AUTH_FAILURE_MESSAGE: &str =
    "Google 账号认证失败，请检查网络代理与登录授权后重试";
const CURSOR_AUTH_ERROR_MESSAGE: &str =
    "Cursor 尚未登录，请在本机终端运行 agent login 后重试（或设置 CURSOR_API_KEY）";
const CURSOR_AUTH_FAILURE_MESSAGE: &str =
    "Cursor 认证失败，请确认本机 Cursor 已登录（agent login）后重试";
const CLAUDE_AUTH_ERROR_MESSAGE: &str =
    "Claude 尚未登录，请先运行 claude login 登录 Claude Code 后重试（或设置 ANTHROPIC_API_KEY）";
const GEMINI_AUTH_ERROR_MESSAGE: &str =
    "Gemini 尚未登录，请在本机终端运行 gemini 完成登录后重试（或设置 GEMINI_API_KEY）";
const COPILOT_AUTH_ERROR_MESSAGE: &str = "Copilot 尚未登录，请在本机终端运行 copilot login 后重试";
const OPENCODE_AUTH_ERROR_MESSAGE: &str = "OpenCode 尚未登录，请先运行 opencode auth login 后重试";
const DEEPSEEK_AUTH_ERROR_MESSAGE: &str = "DeepSeek 尚未配置，请先设置 DEEPSEEK_API_KEY 后重试";

impl AgentProfile {
    pub fn uses_codex_acp_launcher(&self) -> bool {
        self.launcher == Some(LauncherPreset::CodexAcp)
    }

    pub fn uses_antigravity_launcher(&self) -> bool {
        self.launcher == Some(LauncherPreset::AntigravityAcp)
    }

    pub fn injects_codex_cli_env(&self) -> bool {
        self.env_preset == Some(EnvPreset::CodexCli)
    }

    pub fn injects_antigravity_proxy_env(&self) -> bool {
        self.env_preset == Some(EnvPreset::AntigravityProxy)
    }

    pub fn stores_codex_rollouts(&self) -> bool {
        self.session_storage == Some(SessionStoragePreset::CodexRollouts)
    }

    pub fn is_codex_local_auth(&self) -> bool {
        self.auth_policy == Some(AuthPolicy::CodexLocal)
    }

    pub fn is_antigravity_oauth(&self) -> bool {
        self.auth_policy == Some(AuthPolicy::AntigravityOauth)
    }

    pub fn is_cursor_local_auth(&self) -> bool {
        self.auth_policy == Some(AuthPolicy::CursorLocal)
    }

    pub fn is_claude_local_auth(&self) -> bool {
        self.auth_policy == Some(AuthPolicy::ClaudeLocal)
    }

    pub fn is_gemini_local_auth(&self) -> bool {
        self.auth_policy == Some(AuthPolicy::GeminiLocal)
    }

    pub fn is_copilot_local_auth(&self) -> bool {
        self.auth_policy == Some(AuthPolicy::CopilotLocal)
    }

    pub fn is_opencode_local_auth(&self) -> bool {
        self.auth_policy == Some(AuthPolicy::OpenCodeLocal)
    }

    pub fn is_deepseek_key_auth(&self) -> bool {
        self.auth_policy == Some(AuthPolicy::DeepSeekKey)
    }

    pub fn is_claude(&self) -> bool {
        self.kind == AgentKind::Claude || self.id == "claude"
    }

    pub fn is_cursor(&self) -> bool {
        self.is_cursor_local_auth() || self.kind == AgentKind::Cursor || self.id == "cursor"
    }

    pub fn is_codex_like(&self) -> bool {
        self.injects_codex_cli_env() || self.is_codex_local_auth()
    }

    /// Whether this profile has stored local credentials the agent can pick
    /// up on its own. Declared per auth policy (data, not kind): codex-local
    /// reads `~/.codex/auth.json`, antigravity-oauth reads the Gemini
    /// credential files, cursor-local reads the Cursor `auth.json` candidates
    /// or `CURSOR_API_KEY` / `CURSOR_AUTH_TOKEN` env. When present, the ACP
    /// `authenticate` step must be skipped so a stored login is never replaced
    /// by an interactive flow.
    pub fn has_stored_credentials(&self) -> bool {
        match self.auth_policy {
            Some(AuthPolicy::CodexLocal) => crate::agent::discover::codex_auth_present(),
            Some(AuthPolicy::AntigravityOauth) => {
                crate::agent::discover::antigravity_credentials_present()
            }
            Some(AuthPolicy::CursorLocal) => crate::agent::discover::cursor_auth_present(),
            Some(AuthPolicy::ClaudeLocal) => crate::agent::discover::claude_credentials_present(),
            Some(AuthPolicy::GeminiLocal) => crate::agent::discover::gemini_credentials_present(),
            Some(AuthPolicy::CopilotLocal) => crate::agent::discover::copilot_credentials_present(),
            Some(AuthPolicy::OpenCodeLocal) => {
                crate::agent::discover::opencode_credentials_present()
            }
            Some(AuthPolicy::DeepSeekKey) => crate::agent::discover::deepseek_credentials_present(),
            _ => false,
        }
    }

    pub fn is_antigravity(&self) -> bool {
        self.uses_antigravity_launcher()
            || self.injects_antigravity_proxy_env()
            || self.is_antigravity_oauth()
            || self.kind == AgentKind::Antigravity
            || self.id == "antigravity"
    }

    pub fn empty_reply_hint(&self) -> String {
        if self.is_antigravity() {
            ANTIGRAVITY_EMPTY_REPLY_HINT.into()
        } else if self.is_codex_local_auth() {
            CODEX_EMPTY_REPLY_HINT.into()
        } else {
            GENERIC_EMPTY_REPLY_HINT.into()
        }
    }

    pub fn auth_error_message(&self) -> String {
        if self.is_antigravity() {
            ANTIGRAVITY_AUTH_ERROR_MESSAGE.into()
        } else if self.is_codex_local_auth() {
            CODEX_AUTH_ERROR_MESSAGE.into()
        } else if self.is_cursor() {
            CURSOR_AUTH_ERROR_MESSAGE.into()
        } else if self.is_claude_local_auth() {
            CLAUDE_AUTH_ERROR_MESSAGE.into()
        } else if self.is_gemini_local_auth() {
            GEMINI_AUTH_ERROR_MESSAGE.into()
        } else if self.is_copilot_local_auth() {
            COPILOT_AUTH_ERROR_MESSAGE.into()
        } else if self.is_opencode_local_auth() {
            OPENCODE_AUTH_ERROR_MESSAGE.into()
        } else if self.is_deepseek_key_auth() {
            DEEPSEEK_AUTH_ERROR_MESSAGE.into()
        } else {
            GENERIC_AUTH_ERROR_MESSAGE.into()
        }
    }

    /// Fixed business `message` per profile; raw wire text stays in `details`.
    pub fn auth_failure_error(&self, details: Option<&str>) -> AcpError {
        if self.is_antigravity() {
            AcpError::new(
                AcpErrorCode::ProtocolError,
                ANTIGRAVITY_AUTH_FAILURE_MESSAGE,
                details.map(str::to_string),
            )
        } else if self.is_codex_local_auth() {
            AcpError::codex_auth_required(details)
        } else if self.is_cursor() {
            AcpError::new(
                AcpErrorCode::ProtocolError,
                CURSOR_AUTH_FAILURE_MESSAGE,
                details.map(str::to_string),
            )
        } else if self.is_gemini_local_auth()
            || self.is_copilot_local_auth()
            || self.is_opencode_local_auth()
            || self.is_deepseek_key_auth()
            || self.is_claude_local_auth()
        {
            AcpError::new(
                AcpErrorCode::ProtocolError,
                self.auth_error_message(),
                details.map(str::to_string),
            )
        } else {
            AcpError::protocol(details)
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreparedProfiles {
    pub active_profile_id: String,
    pub profiles: Vec<AgentProfile>,
}

pub fn default_profiles_hint() -> AgentProfilesHint {
    AgentProfilesHint {
        active_profile_id: "codex".into(),
        profiles: builtin_profiles()
            .into_iter()
            .map(|profile| AgentProfileInput {
                id: profile.id,
                name: profile.name,
                kind: profile.kind,
                command: profile.command,
                args: profile.args,
                env: profile.env,
                launcher: profile.launcher,
                env_preset: profile.env_preset,
                auth_policy: profile.auth_policy,
                auth_methods: profile.auth_methods,
                session_storage: profile.session_storage,
            })
            .collect(),
    }
}

pub fn prepare_profiles(hint: &AgentProfilesHint) -> PreparedProfiles {
    let mut active_id = if hint.active_profile_id.trim().is_empty() {
        "codex".to_string()
    } else {
        hint.active_profile_id.clone()
    };
    let mut profiles: Vec<AgentProfile> = if hint.profiles.is_empty() {
        builtin_profiles()
    } else {
        hint.profiles.iter().map(profile_from_input).collect()
    };
    merge_builtin_profiles(&mut profiles);
    if !profiles.iter().any(|p| p.id == active_id) {
        active_id = "codex".to_string();
    }
    PreparedProfiles {
        active_profile_id: active_id,
        profiles,
    }
}

pub fn list_status(prepared: &PreparedProfiles) -> (String, Vec<AgentProfileStatus>) {
    let statuses = prepared
        .profiles
        .iter()
        .map(profile_status)
        .collect::<Vec<_>>();
    (prepared.active_profile_id.clone(), statuses)
}

pub fn resolve_active_profile(
    prepared: &PreparedProfiles,
    override_id: Option<&str>,
) -> Result<AgentProfile, AcpError> {
    let id = override_id.unwrap_or(&prepared.active_profile_id);
    prepared
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| AcpError::bad_request(format!("找不到该 Agent 配置：{id}")))
}

fn profile_from_input(input: &AgentProfileInput) -> AgentProfile {
    AgentProfile {
        id: input.id.clone(),
        name: input.name.clone(),
        kind: input.kind,
        command: input.command.clone(),
        args: input.args.clone(),
        env: input.env.clone(),
        launcher: input.launcher,
        env_preset: input.env_preset,
        auth_policy: input.auth_policy,
        auth_methods: input.auth_methods.clone(),
        session_storage: input.session_storage,
    }
}

fn profile_status(profile: &AgentProfile) -> AgentProfileStatus {
    let resolved = resolve_launch(profile).ok();
    AgentProfileStatus {
        id: profile.id.clone(),
        name: profile.name.clone(),
        kind: profile.kind,
        command: profile.command.clone(),
        args: profile.args.clone(),
        env: profile.env.clone(),
        available: resolved.is_some(),
        resolved_command: resolved.map(|launch| launch.program.to_string_lossy().to_string()),
    }
}

fn builtin_profiles() -> Vec<AgentProfile> {
    vec![
        builtin_codex(),
        builtin_antigravity(),
        builtin_claude(),
        builtin_cursor(),
        builtin_gemini(),
        builtin_copilot(),
        builtin_opencode(),
        builtin_deepseek(),
        builtin_custom_template(),
    ]
}

fn builtin_antigravity() -> AgentProfile {
    let mut env = HashMap::new();
    env.insert("ACP_PROXY_PORT".into(), "7897".into());
    AgentProfile {
        id: "antigravity".into(),
        name: "Google Antigravity".into(),
        kind: AgentKind::Antigravity,
        command: if cfg!(windows) {
            "agy_acp_server.exe".into()
        } else {
            "agy_acp_server".into()
        },
        args: Vec::new(),
        env,
        launcher: Some(LauncherPreset::AntigravityAcp),
        env_preset: Some(EnvPreset::AntigravityProxy),
        auth_policy: Some(AuthPolicy::AntigravityOauth),
        auth_methods: vec!["oauth-personal".into()],
        session_storage: None,
    }
}

fn builtin_codex() -> AgentProfile {
    AgentProfile {
        id: "codex".into(),
        name: "Codex（默认）".into(),
        kind: AgentKind::Codex,
        command: if cfg!(windows) {
            "bunx.exe".into()
        } else {
            "bunx".into()
        },
        args: vec![CODEX_ACP_PACKAGE.into()],
        env: HashMap::new(),
        launcher: Some(LauncherPreset::CodexAcp),
        env_preset: Some(EnvPreset::CodexCli),
        auth_policy: Some(AuthPolicy::CodexLocal),
        auth_methods: Vec::new(),
        session_storage: Some(SessionStoragePreset::CodexRollouts),
    }
}

fn builtin_claude() -> AgentProfile {
    AgentProfile {
        id: "claude".into(),
        name: "Claude ACP".into(),
        kind: AgentKind::Claude,
        command: if cfg!(windows) {
            "claude-agent-acp.exe".into()
        } else {
            "claude-agent-acp".into()
        },
        args: Vec::new(),
        env: HashMap::new(),
        launcher: None,
        env_preset: None,
        // 官方渠道本地认证复用：Claude Code 登录（`claude login`）或
        // ANTHROPIC_API_KEY；命中则跳过 ACP authenticate。
        auth_policy: Some(AuthPolicy::ClaudeLocal),
        auth_methods: Vec::new(),
        session_storage: None,
    }
}

fn builtin_cursor() -> AgentProfile {
    AgentProfile {
        id: "cursor".into(),
        name: "Cursor CLI".into(),
        kind: AgentKind::Cursor,
        command: if cfg!(windows) {
            "agent.cmd".into()
        } else {
            "agent".into()
        },
        args: vec!["acp".into()],
        env: HashMap::new(),
        launcher: None,
        env_preset: None,
        // 本地已有认证复用：`agent login` 凭据或 CURSOR_API_KEY /
        // CURSOR_AUTH_TOKEN 透传；命中则跳过 ACP authenticate。
        auth_policy: Some(AuthPolicy::CursorLocal),
        auth_methods: Vec::new(),
        // 通用 ACP session/list + hint resume，不读任何厂商私有落盘。
        session_storage: None,
    }
}

fn builtin_gemini() -> AgentProfile {
    AgentProfile {
        id: "gemini".into(),
        name: "Gemini CLI".into(),
        kind: AgentKind::Gemini,
        command: if cfg!(windows) {
            "gemini.cmd".into()
        } else {
            "gemini".into()
        },
        args: vec!["--acp".into()],
        env: HashMap::new(),
        launcher: None,
        env_preset: None,
        // 本地已有认证复用：`gemini` 登录或 GEMINI_API_KEY /
        // GOOGLE_API_KEY 透传；命中则跳过 ACP authenticate。
        auth_policy: Some(AuthPolicy::GeminiLocal),
        auth_methods: Vec::new(),
        session_storage: None,
    }
}

fn builtin_copilot() -> AgentProfile {
    AgentProfile {
        id: "copilot".into(),
        name: "Copilot CLI".into(),
        kind: AgentKind::Copilot,
        command: if cfg!(windows) {
            "copilot.cmd".into()
        } else {
            "copilot".into()
        },
        args: vec!["--acp".into(), "--stdio".into()],
        env: HashMap::new(),
        launcher: None,
        env_preset: None,
        // 本地已有认证复用：`copilot login` 的 GitHub 登录或
        // COPILOT_GITHUB_TOKEN / GH_TOKEN / GITHUB_TOKEN 透传。
        auth_policy: Some(AuthPolicy::CopilotLocal),
        auth_methods: Vec::new(),
        session_storage: None,
    }
}

fn builtin_opencode() -> AgentProfile {
    AgentProfile {
        id: "opencode".into(),
        name: "OpenCode".into(),
        kind: AgentKind::OpenCode,
        command: if cfg!(windows) {
            "opencode.exe".into()
        } else {
            "opencode".into()
        },
        args: vec!["acp".into()],
        env: HashMap::new(),
        launcher: None,
        env_preset: None,
        // 本地已有认证复用：`opencode auth login` 写入的 auth.json。
        auth_policy: Some(AuthPolicy::OpenCodeLocal),
        auth_methods: Vec::new(),
        session_storage: None,
    }
}

fn builtin_deepseek() -> AgentProfile {
    AgentProfile {
        id: "deepseek".into(),
        name: "DeepSeek Harness".into(),
        kind: AgentKind::DeepSeek,
        command: if cfg!(windows) {
            "bunx.exe".into()
        } else {
            "bunx".into()
        },
        args: vec![
            "-y".into(),
            "@deepseek-ai/dsh".into(),
            "--profile".into(),
            "acp".into(),
        ],
        env: HashMap::new(),
        launcher: None,
        env_preset: None,
        // 无 ACP 登录：harness 自读 DEEPSEEK_API_KEY / 自身配置。
        auth_policy: Some(AuthPolicy::DeepSeekKey),
        auth_methods: Vec::new(),
        session_storage: None,
    }
}

fn builtin_custom_template() -> AgentProfile {
    AgentProfile {
        id: "custom".into(),
        name: "自定义 ACP".into(),
        kind: AgentKind::Custom,
        command: String::new(),
        args: Vec::new(),
        env: HashMap::new(),
        launcher: None,
        env_preset: None,
        auth_policy: None,
        auth_methods: Vec::new(),
        session_storage: None,
    }
}

fn merge_builtin_profiles(profiles: &mut Vec<AgentProfile>) {
    for builtin in builtin_profiles() {
        if builtin.id == "custom" && builtin.command.is_empty() {
            if !profiles.iter().any(|profile| profile.id == "custom") {
                profiles.push(builtin);
            }
            continue;
        }
        if builtin.id == "antigravity" {
            if let Some(existing) = profiles
                .iter_mut()
                .find(|profile| profile.id == "antigravity")
            {
                if existing.kind == AgentKind::Antigravity {
                    apply_missing_antigravity_presets(existing);
                    continue;
                }
            }
            if !profiles.iter().any(|profile| profile.id == "antigravity") {
                profiles.push(builtin);
            }
            continue;
        }
        if builtin.id == "codex" {
            if let Some(existing) = profiles.iter_mut().find(|profile| profile.id == "codex") {
                let legacy_command =
                    existing.command == "codex-acp" || existing.command == "codex-acp.exe";
                if existing.kind == AgentKind::Codex
                    && legacy_command
                    && existing.args.is_empty()
                    && existing.env.is_empty()
                {
                    *existing = builtin;
                } else if existing.kind == AgentKind::Codex && is_default_codex_command(existing) {
                    apply_missing_codex_presets(existing);
                }
                continue;
            }
        }
        if !profiles.iter().any(|profile| profile.id == builtin.id) {
            profiles.push(builtin);
        }
    }
}

/// The `bunx @agentclientprotocol/codex-acp` shape that the built-in preset
/// ships with. Only this shape receives implicit presets; a user-customized
/// codex command keeps fully generic behavior (as before this refactor).
fn is_default_codex_command(profile: &AgentProfile) -> bool {
    matches!(profile.command.as_str(), "bunx" | "bunx.exe")
        && profile.args.len() == 1
        && profile.args.first().map(String::as_str) == Some(CODEX_ACP_PACKAGE)
}

fn apply_missing_codex_presets(profile: &mut AgentProfile) {
    let codex = builtin_codex();
    if profile.launcher.is_none() {
        profile.launcher = codex.launcher;
    }
    if profile.env_preset.is_none() {
        profile.env_preset = codex.env_preset;
    }
    if profile.auth_policy.is_none() {
        profile.auth_policy = codex.auth_policy;
    }
    if profile.auth_methods.is_empty() {
        profile.auth_methods = codex.auth_methods;
    }
    if profile.session_storage.is_none() {
        profile.session_storage = codex.session_storage;
    }
}

fn apply_missing_antigravity_presets(profile: &mut AgentProfile) {
    let antigravity = builtin_antigravity();
    if profile.launcher.is_none() {
        profile.launcher = antigravity.launcher;
    }
    if profile.env_preset.is_none() {
        profile.env_preset = antigravity.env_preset;
    }
    if profile.auth_policy.is_none() {
        profile.auth_policy = antigravity.auth_policy;
    }
    if profile.auth_methods.is_empty() {
        profile.auth_methods = antigravity.auth_methods;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hint_has_codex_active() {
        let hint = default_profiles_hint();
        assert_eq!(hint.active_profile_id, "codex");
        let codex = hint
            .profiles
            .iter()
            .find(|profile| profile.id == "codex")
            .expect("codex");
        assert!(codex.command.contains("bunx"));
        assert_eq!(codex.args, vec![CODEX_ACP_PACKAGE]);
        assert!(hint
            .profiles
            .iter()
            .any(|profile| profile.id == "antigravity"));
        assert!(hint.profiles.iter().any(|profile| profile.id == "claude"));
    }

    #[test]
    fn prepare_and_resolve_custom_profile() {
        let mut hint = default_profiles_hint();
        hint.profiles = hint
            .profiles
            .into_iter()
            .map(|mut profile| {
                if profile.id == "custom" {
                    profile.command = "nonexistent-agent-xyz".into();
                }
                profile
            })
            .collect();
        hint.active_profile_id = "custom".into();
        let prepared = prepare_profiles(&hint);
        let active = resolve_active_profile(&prepared, None).expect("load");
        assert_eq!(active.id, "custom");
    }

    #[test]
    fn builtin_codex_carries_all_presets() {
        let codex = builtin_codex();
        assert!(codex.uses_codex_acp_launcher());
        assert!(codex.injects_codex_cli_env());
        assert!(codex.is_codex_local_auth());
        assert!(codex.stores_codex_rollouts());
        assert_eq!(codex.empty_reply_hint(), CODEX_EMPTY_REPLY_HINT);
        assert_eq!(codex.auth_error_message(), CODEX_AUTH_ERROR_MESSAGE);
    }

    #[test]
    fn builtin_cursor_reuses_local_login_without_codex_presets() {
        let cursor = builtin_cursor();
        assert_eq!(cursor.id, "cursor");
        assert_eq!(cursor.kind, AgentKind::Cursor);
        assert_eq!(cursor.args, vec!["acp"]);
        assert!(cursor.is_cursor());
        assert!(cursor.is_cursor_local_auth());
        // 绝不继承 codex 的 env/存储：各家凭据各家读。
        assert!(!cursor.injects_codex_cli_env());
        assert!(!cursor.is_codex_local_auth());
        assert!(!cursor.stores_codex_rollouts());
        assert!(cursor.session_storage.is_none());
        assert_eq!(cursor.auth_error_message(), CURSOR_AUTH_ERROR_MESSAGE);
    }

    #[test]
    fn default_hint_and_merge_include_cursor() {
        let hint = default_profiles_hint();
        assert!(hint.profiles.iter().any(|profile| profile.id == "cursor"));
        // 旧落盘缺 cursor 时自动补齐（frontend mergeProfiles 同理）。
        let mut hint_without_cursor = hint.clone();
        hint_without_cursor
            .profiles
            .retain(|profile| profile.id != "cursor");
        let prepared = prepare_profiles(&hint_without_cursor);
        let cursor = prepared
            .profiles
            .iter()
            .find(|profile| profile.id == "cursor")
            .expect("cursor merged");
        assert!(cursor.is_cursor_local_auth());
    }

    #[test]
    fn builtin_claude_uses_official_local_auth() {
        let claude = builtin_claude();
        assert_eq!(claude.kind, AgentKind::Claude);
        assert!(claude.is_claude());
        assert!(claude.is_claude_local_auth());
        assert!(!claude.injects_codex_cli_env());
        assert!(!claude.is_codex_local_auth());
        assert!(!claude.stores_codex_rollouts());
        assert_eq!(claude.auth_error_message(), CLAUDE_AUTH_ERROR_MESSAGE);
    }

    #[test]
    fn builtin_agent_lineup_covers_all_runtimes() {
        let hint = default_profiles_hint();
        let ids: Vec<&str> = hint
            .profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect();
        for expected in [
            "codex",
            "antigravity",
            "claude",
            "cursor",
            "gemini",
            "copilot",
            "opencode",
            "deepseek",
            "custom",
        ] {
            assert!(ids.contains(&expected), "missing builtin {expected}");
        }
        let prepared = prepare_profiles(&hint);
        let by_id = |id: &str| {
            prepared
                .profiles
                .iter()
                .find(|profile| profile.id == id)
                .expect(id)
        };
        assert!(by_id("gemini").is_gemini_local_auth());
        assert!(by_id("claude").is_claude_local_auth());
        assert_eq!(by_id("gemini").args, vec!["--acp"]);
        assert!(by_id("copilot").is_copilot_local_auth());
        assert_eq!(by_id("copilot").args, vec!["--acp", "--stdio"]);
        assert!(by_id("opencode").is_opencode_local_auth());
        assert_eq!(by_id("opencode").args, vec!["acp"]);
        assert!(by_id("deepseek").is_deepseek_key_auth());
        // 各家只认自家 policy，绝不继承 codex 的 env/存储。
        for profile in prepared.profiles.iter() {
            if profile.id == "codex" {
                continue;
            }
            assert!(
                !profile.injects_codex_cli_env(),
                "{} must not inject codex env",
                profile.id
            );
            assert!(
                !profile.stores_codex_rollouts(),
                "{} must not read codex rollouts",
                profile.id
            );
        }
        assert_eq!(
            by_id("gemini").auth_error_message(),
            GEMINI_AUTH_ERROR_MESSAGE
        );
        assert_eq!(
            by_id("copilot").auth_error_message(),
            COPILOT_AUTH_ERROR_MESSAGE
        );
        assert_eq!(
            by_id("opencode").auth_error_message(),
            OPENCODE_AUTH_ERROR_MESSAGE
        );
        assert_eq!(
            by_id("deepseek").auth_error_message(),
            DEEPSEEK_AUTH_ERROR_MESSAGE
        );
    }

    #[test]
    fn non_codex_profiles_carry_no_codex_behavior() {
        for profile in builtin_profiles() {
            if profile.id == "codex" || profile.id == "antigravity" {
                continue;
            }
            assert!(!profile.uses_codex_acp_launcher());
            assert!(!profile.injects_codex_cli_env());
            assert!(!profile.is_codex_local_auth());
            assert!(!profile.stores_codex_rollouts());
        }
    }

    #[test]
    fn fully_generic_profiles_skip_auth_and_use_generic_copy() {
        // 只有无 policy 的画像才断言 stored-credentials（带 policy 的探针读
        // 真实 env/文件，环境相关，不进确定性单测）。目前仅 custom 全 generic。
        let profile = builtin_profiles()
            .into_iter()
            .find(|item| item.id == "custom")
            .expect("custom");
        assert!(!profile.has_stored_credentials());
        assert_eq!(profile.empty_reply_hint(), GENERIC_EMPTY_REPLY_HINT);
    }

    #[test]
    fn builtin_antigravity_profile_properties() {
        let agy = builtin_antigravity();
        assert!(agy.is_antigravity());
        assert!(agy.uses_antigravity_launcher());
        assert!(agy.injects_antigravity_proxy_env());
        assert!(agy.is_antigravity_oauth());
        assert!(!agy.uses_codex_acp_launcher());
        assert!(!agy.injects_codex_cli_env());
        assert!(!agy.is_codex_local_auth());
        assert_eq!(agy.empty_reply_hint(), ANTIGRAVITY_EMPTY_REPLY_HINT);
        assert_eq!(agy.auth_error_message(), ANTIGRAVITY_AUTH_ERROR_MESSAGE);
    }

    #[test]
    fn legacy_antigravity_hint_without_presets_gets_presets_backfilled() {
        let mut hint = default_profiles_hint();
        hint.profiles = hint
            .profiles
            .into_iter()
            .map(|mut profile| {
                if profile.id == "antigravity" {
                    profile.launcher = None;
                    profile.env_preset = None;
                    profile.auth_policy = None;
                }
                profile
            })
            .collect();
        let prepared = prepare_profiles(&hint);
        let agy = prepared
            .profiles
            .iter()
            .find(|profile| profile.id == "antigravity")
            .expect("antigravity");
        assert!(agy.uses_antigravity_launcher());
        assert!(agy.injects_antigravity_proxy_env());
        assert!(agy.is_antigravity_oauth());
    }

    #[test]
    fn legacy_codex_hint_without_presets_gets_presets_backfilled() {
        let mut hint = default_profiles_hint();
        hint.profiles = hint
            .profiles
            .into_iter()
            .map(|mut profile| {
                if profile.id == "codex" {
                    profile.launcher = None;
                    profile.env_preset = None;
                    profile.auth_policy = None;
                    profile.session_storage = None;
                }
                profile
            })
            .collect();
        let prepared = prepare_profiles(&hint);
        let codex = prepared
            .profiles
            .iter()
            .find(|profile| profile.id == "codex")
            .expect("codex");
        assert!(codex.uses_codex_acp_launcher());
        assert!(codex.injects_codex_cli_env());
        assert!(codex.is_codex_local_auth());
        assert!(codex.stores_codex_rollouts());
    }

    #[test]
    fn user_customized_codex_command_stays_generic() {
        let mut hint = default_profiles_hint();
        hint.profiles = hint
            .profiles
            .into_iter()
            .map(|mut profile| {
                if profile.id == "codex" {
                    profile.command = "C:\\tools\\codex-acp.exe".into();
                    profile.args = Vec::new();
                    profile.launcher = None;
                    profile.env_preset = None;
                    profile.auth_policy = None;
                    profile.session_storage = None;
                }
                profile
            })
            .collect();
        let prepared = prepare_profiles(&hint);
        let codex = prepared
            .profiles
            .iter()
            .find(|profile| profile.id == "codex")
            .expect("codex");
        assert!(!codex.uses_codex_acp_launcher());
        assert!(!codex.injects_codex_cli_env());
        assert!(!codex.is_codex_local_auth());
    }

    #[test]
    fn profile_input_deserializes_without_new_fields() {
        let json = r#"{
            "id": "codex",
            "name": "Codex（默认）",
            "kind": "Codex",
            "command": "bunx.exe",
            "args": ["@agentclientprotocol/codex-acp"]
        }"#;
        let input: AgentProfileInput = serde_json::from_str(json).expect("legacy profile json");
        assert!(input.launcher.is_none());
        assert!(input.env_preset.is_none());
        assert!(input.auth_policy.is_none());
        assert!(input.session_storage.is_none());
        assert!(input.auth_methods.is_empty());
    }
}
