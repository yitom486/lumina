//! Pluggable ACP Agent profiles (default: Codex adapter → App Server).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::acp::discover::{find_acp_adapter, find_bunx, find_codex, find_command, native_acp_dir};
use crate::acp::error::AcpError;

pub const CODEX_ACP_PACKAGE: &str = "@agentclientprotocol/codex-acp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AgentKind {
    Codex,
    Claude,
    Custom,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileStatus {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub available: bool,
    pub resolved_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentsFile {
    active_profile_id: String,
    profiles: Vec<AgentProfile>,
}

pub struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub fn new() -> Self {
        Self {
            path: default_config_path(),
        }
    }

    #[cfg(test)]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    fn load_or_default(&self) -> Result<AgentsFile, AcpError> {
        if !self.path.exists() {
            return Ok(default_agents_file());
        }
        let raw = fs::read_to_string(&self.path).map_err(|error| {
            tracing::warn!(%error, "failed to read agent config");
            AcpError::internal(Some(&format!("read agent config: {error}")))
        })?;
        if raw.trim().is_empty() {
            return Ok(default_agents_file());
        }
        let mut file: AgentsFile = serde_json::from_str(&raw).map_err(|error| {
            tracing::warn!(%error, "invalid agent config json");
            AcpError::internal(Some(&format!("parse agent config: {error}")))
        })?;
        merge_builtin_profiles(&mut file);
        Ok(file)
    }

    fn save(&self, file: &AgentsFile) -> Result<(), AcpError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                tracing::warn!(%error, "failed to create agent config dir");
                AcpError::internal(Some(&format!("create agent config dir: {error}")))
            })?;
        }
        let raw = serde_json::to_string_pretty(file).map_err(|error| {
            AcpError::internal(Some(&format!("serialize agent config: {error}")))
        })?;
        fs::write(&self.path, raw).map_err(|error| {
            tracing::warn!(%error, "failed to write agent config");
            AcpError::internal(Some(&format!("write agent config: {error}")))
        })?;
        Ok(())
    }

    pub fn list_status(&self) -> Result<(String, Vec<AgentProfileStatus>), AcpError> {
        let file = self.load_or_default()?;
        let statuses = file.profiles.iter().map(profile_status).collect::<Vec<_>>();
        Ok((file.active_profile_id, statuses))
    }

    pub fn active_profile(&self) -> Result<AgentProfile, AcpError> {
        let file = self.load_or_default()?;
        file.profiles
            .into_iter()
            .find(|p| p.id == file.active_profile_id)
            .or_else(|| Some(builtin_codex()))
            .ok_or_else(|| AcpError::not_configured(Some("no active agent profile")))
    }

    pub fn set_active(&self, id: &str) -> Result<(), AcpError> {
        let mut file = self.load_or_default()?;
        if !file.profiles.iter().any(|p| p.id == id) {
            return Err(AcpError::bad_request(format!("找不到该 Agent 配置：{id}")));
        }
        file.active_profile_id = id.to_string();
        self.save(&file)
    }

    pub fn upsert(&self, profile: AgentProfile) -> Result<AgentProfile, AcpError> {
        if profile.id.trim().is_empty() || profile.command.trim().is_empty() {
            return Err(AcpError::bad_request("Agent id 与 command 不能为空"));
        }
        let mut file = self.load_or_default()?;
        if let Some(existing) = file.profiles.iter_mut().find(|p| p.id == profile.id) {
            *existing = profile.clone();
        } else {
            file.profiles.push(profile.clone());
        }
        self.save(&file)?;
        Ok(profile)
    }
}

impl Default for ProfileStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub display_name: String,
}

pub fn resolve_launch(profile: &AgentProfile) -> Result<LaunchSpec, AcpError> {
    let (program, args) = if is_builtin_codex(profile) {
        if let Some(bunx) = find_bunx() {
            (bunx, profile.args.clone())
        } else if let Some(adapter) = find_acp_adapter() {
            // Packaged installations may ship a standalone adapter and no Bun.
            (adapter, Vec::new())
        } else {
            return Err(AcpError::not_configured(Some(
                "codex profile requires bunx or a standalone codex-acp adapter",
            )));
        }
    } else {
        let program = resolve_program(&profile.command).ok_or_else(|| {
            AcpError::not_configured(Some(&format!(
                "agent `{}` command not found: {}",
                profile.id, profile.command
            )))
        })?;
        (program, profile.args.clone())
    };

    let mut env = profile.env.clone();
    if profile.kind == AgentKind::Codex {
        if !env.contains_key("CODEX_PATH") {
            if let Some(codex) = find_codex() {
                env.insert("CODEX_PATH".into(), codex.to_string_lossy().to_string());
            }
        }
    }

    Ok(LaunchSpec {
        program,
        args,
        env,
        display_name: profile.name.clone(),
    })
}

fn is_builtin_codex(profile: &AgentProfile) -> bool {
    profile.id == "codex"
        && profile.kind == AgentKind::Codex
        && matches!(profile.command.as_str(), "bunx" | "bunx.exe")
        && profile.args.first().map(String::as_str) == Some(CODEX_ACP_PACKAGE)
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

fn resolve_program(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.is_absolute() || command.contains('/') || command.contains('\\') {
        return path.is_file().then(|| path.to_path_buf());
    }
    find_command(command)
}

fn default_agents_file() -> AgentsFile {
    AgentsFile {
        active_profile_id: "codex".into(),
        profiles: builtin_profiles(),
    }
}

fn builtin_profiles() -> Vec<AgentProfile> {
    vec![builtin_codex(), builtin_claude(), builtin_custom_template()]
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
    }
}

fn merge_builtin_profiles(file: &mut AgentsFile) {
    for builtin in builtin_profiles() {
        if builtin.id == "custom" && builtin.command.is_empty() {
            // Keep user's custom if present; ensure slot exists.
            if !file.profiles.iter().any(|p| p.id == "custom") {
                file.profiles.push(builtin);
            }
            continue;
        }
        if builtin.id == "codex" {
            if let Some(existing) = file.profiles.iter_mut().find(|p| p.id == "codex") {
                // Migrate only the previous untouched built-in profile. User-edited
                // Codex commands remain authoritative.
                let legacy_command =
                    existing.command == "codex-acp" || existing.command == "codex-acp.exe";
                if existing.kind == AgentKind::Codex
                    && legacy_command
                    && existing.args.is_empty()
                    && existing.env.is_empty()
                {
                    *existing = builtin;
                }
                continue;
            }
        }
        if !file.profiles.iter().any(|p| p.id == builtin.id) {
            file.profiles.push(builtin);
        }
    }
    if file.active_profile_id.is_empty() {
        file.active_profile_id = "codex".into();
    }
}

fn default_config_path() -> PathBuf {
    if let Some(dir) = data_dir() {
        return dir.join("lumina").join("acp-agents.json");
    }
    std::env::temp_dir().join("lumina-acp-agents.json")
}

pub(crate) fn data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
    }
}

/// Hint text: install guidance without requiring bun for end users.
pub fn install_hint(adapter_found: bool, codex_found: bool, bunx_found: bool) -> String {
    if bunx_found {
        return if codex_found {
            "将通过 Bun 按需获取并启动 Codex ACP；已找到本机 Codex，也可通过 CODEX_PATH 指定版本。首次使用可能需要下载适配器。".into()
        } else {
            "将通过 Bun 按需获取并启动 Codex ACP；发布包自带兼容的 Codex。首次使用可能需要下载适配器。".into()
        };
    }
    if adapter_found {
        if codex_found {
            return "Codex ACP 适配器与 Codex 均已找到。模型侧请使用 Responses API（非 Chat Completions）。".into();
        }
        return "已找到 ACP 适配器；未找到 Codex 本体时，适配器可能仍能使用自带/环境中的 Codex。可选安装官方 Codex，或设置 CODEX_PATH。".into();
    }
    format!(
        "未找到可用 ACP Agent。请安装 Bun，或将 codex-acp 单文件放到 {}。播放功能不依赖这些可选组件。",
        native_acp_dir().display()
    )
}

pub const RESPONSES_ONLY_NOTE: &str =
    "Codex 自定义模型须使用 Responses API（wire_api=responses）。仅支持 Chat Completions 的地址需经 LiteLLM/OpenRouter 等网关，或改用其它 ACP Agent（如 Claude）。";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_has_codex_active() {
        let file = default_agents_file();
        assert_eq!(file.active_profile_id, "codex");
        let codex = file
            .profiles
            .iter()
            .find(|p| p.id == "codex")
            .expect("codex");
        assert!(codex.command.contains("bunx"));
        assert_eq!(codex.args, vec![CODEX_ACP_PACKAGE]);
        assert!(file.profiles.iter().any(|p| p.id == "claude"));
    }

    #[test]
    fn upsert_and_set_active_roundtrip() {
        let path = std::env::temp_dir().join("lumina-acp-agents-test.json");
        let _ = fs::remove_file(&path);
        let store = ProfileStore::with_path(path.clone());
        let mut custom = builtin_custom_template();
        custom.command = "nonexistent-agent-xyz".into();
        store.upsert(custom).expect("upsert");
        store.set_active("custom").expect("active");
        let active = store.active_profile().expect("load");
        assert_eq!(active.id, "custom");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn install_hint_is_chinese() {
        let hint = install_hint(false, false, false);
        assert!(hint.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
        assert!(!hint.to_ascii_lowercase().contains("must install bun"));
    }

    #[test]
    fn bunx_hint_describes_on_demand_launch() {
        let hint = install_hint(false, true, true);
        assert!(hint.contains("Bun"));
        assert!(hint.contains("按需"));
        assert!(!hint.contains("未找到可用"));
    }
}
