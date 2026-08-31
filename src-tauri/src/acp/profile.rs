//! Pluggable ACP Agent profiles (default: Codex adapter → App Server).
//! Profiles are persisted in the frontend (Zustand); Rust only receives hints per invoke.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::acp::discover::{find_acp_adapter, find_bunx, find_codex, find_command, native_acp_dir};
use crate::acp::error::AcpError;
use crate::acp::model::{AgentProfileInput, AgentProfilesHint};

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

fn merge_builtin_profiles(profiles: &mut Vec<AgentProfile>) {
    for builtin in builtin_profiles() {
        if builtin.id == "custom" && builtin.command.is_empty() {
            if !profiles.iter().any(|profile| profile.id == "custom") {
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
                }
                continue;
            }
        }
        if !profiles.iter().any(|profile| profile.id == builtin.id) {
            profiles.push(builtin);
        }
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
