//! Pluggable ACP Agent profiles (default: Codex adapter → App Server).
//! Profiles are persisted in the frontend (Zustand); Rust only receives hints per invoke.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use crate::domain::model::{AgentKind, AgentProfileStatus};
use crate::domain::model::{AgentProfileInput, AgentProfilesHint};
use crate::error::AcpError;

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
}
