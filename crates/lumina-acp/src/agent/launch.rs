//! Agent launch resolution (program + args + env).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent::discover::{
    codex_home_dir, find_acp_adapter, find_bun, find_bunx, find_codex, find_command,
    find_dev_codex_acp_entry,
};
use crate::error::AcpError;

use super::profile::{AgentKind, AgentProfile};

pub const CODEX_ACP_PACKAGE: &str = "@agentclientprotocol/codex-acp";

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub display_name: String,
}

pub fn resolve_launch(profile: &AgentProfile) -> Result<LaunchSpec, AcpError> {
    let (program, args) = if is_builtin_codex(profile) {
        resolve_builtin_codex_launch(profile)?
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
    if profile.kind == AgentKind::Codex && !env.contains_key("CODEX_PATH") {
        if let Some(codex) = find_codex() {
            env.insert("CODEX_PATH".into(), codex.to_string_lossy().to_string());
        }
    }
    augment_spawn_env(profile.kind, &mut env);

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

fn resolve_program(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.is_absolute() || command.contains('/') || command.contains('\\') {
        return path.is_file().then(|| path.to_path_buf());
    }
    find_command(command)
}

fn resolve_builtin_codex_launch(
    profile: &AgentProfile,
) -> Result<(PathBuf, Vec<String>), AcpError> {
    if let Some(adapter) = find_acp_adapter() {
        return Ok((adapter, Vec::new()));
    }
    if let Some(entry) = find_dev_codex_acp_entry() {
        if let Some(bun) = find_bun() {
            return Ok((bun, vec!["run".into(), entry.to_string_lossy().to_string()]));
        }
    }
    // Windows: `bun x pkg` is more reliable than `bunx pkg` when stdio is piped.
    if cfg!(windows) {
        if let Some(bun) = find_bun() {
            return Ok((bun, vec!["x".into(), CODEX_ACP_PACKAGE.into()]));
        }
    }
    if let Some(bunx) = find_bunx() {
        return Ok((bunx, profile.args.clone()));
    }
    Err(AcpError::not_configured(Some(
        "codex profile requires bun, bunx, or a standalone codex-acp adapter",
    )))
}

fn augment_spawn_env(kind: AgentKind, env: &mut HashMap<String, String>) {
    if cfg!(windows) {
        if !env.contains_key("USERPROFILE") {
            if let Ok(value) = std::env::var("USERPROFILE") {
                env.insert("USERPROFILE".into(), value);
            }
        }
    } else if !env.contains_key("HOME") {
        if let Ok(value) = std::env::var("HOME") {
            env.insert("HOME".into(), value);
        }
    }

    if kind != AgentKind::Codex {
        return;
    }

    if let Some(home) = codex_home_dir() {
        env.entry("CODEX_HOME".into())
            .or_insert(home.to_string_lossy().to_string());
    }
    env.entry("TERM".into()).or_insert("xterm-256color".into());

    if let Some(codex_path) = env
        .get("CODEX_PATH")
        .cloned()
        .or_else(|| find_codex().map(|path| path.to_string_lossy().to_string()))
    {
        if let Some(bin) = PathBuf::from(&codex_path).parent() {
            prepend_path_dir(env, bin);
        }
    }

    let path_key = if cfg!(windows) { "Path" } else { "PATH" };
    let mut extra = Vec::new();
    if let Some(bunx) = find_bunx() {
        if let Some(bin) = bunx.parent() {
            extra.push(bin.to_path_buf());
        }
    }
    #[cfg(windows)]
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let home = PathBuf::from(home);
        extra.push(home.join(".bun").join("bin"));
        extra.push(home.join("AppData").join("Roaming").join("npm"));
    }
    if let Ok(existing) = std::env::var(path_key) {
        let mut merged: Vec<PathBuf> = extra;
        merged.extend(std::env::split_paths(&existing));
        if let Ok(joined) = std::env::join_paths(merged) {
            env.insert(path_key.into(), joined.to_string_lossy().to_string());
        }
    } else if !extra.is_empty() {
        if let Ok(joined) = std::env::join_paths(extra) {
            env.insert(path_key.into(), joined.to_string_lossy().to_string());
        }
    }
}

fn prepend_path_dir(env: &mut HashMap<String, String>, dir: &Path) {
    let path_key = if cfg!(windows) { "Path" } else { "PATH" };
    let mut merged = vec![dir.to_path_buf()];
    if let Some(existing) = env
        .get(path_key)
        .cloned()
        .or_else(|| std::env::var(path_key).ok())
    {
        merged.extend(std::env::split_paths(&existing));
    }
    if let Ok(joined) = std::env::join_paths(merged) {
        env.insert(path_key.into(), joined.to_string_lossy().to_string());
    }
}
