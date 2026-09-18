//! Agent launch resolution (program + args + env).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent::discover::{
    codex_config_present, codex_home_dir, find_acp_adapter, find_antigravity, find_bun, find_bunx,
    find_codex, find_command, find_dev_codex_acp_entry,
};
use crate::error::AcpError;
use crate::wire::session::{AuthMethod, InitializeResult};

use super::profile::AgentProfile;

pub const CODEX_ACP_PACKAGE: &str = "@agentclientprotocol/codex-acp";

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub display_name: String,
}

pub fn resolve_launch(profile: &AgentProfile) -> Result<LaunchSpec, AcpError> {
    let (program, args) = if profile.uses_codex_acp_launcher() {
        resolve_codex_acp_fallback(profile)?
    } else if profile.uses_antigravity_launcher() {
        let program = resolve_program(&profile.command)
            .or_else(find_antigravity)
            .ok_or_else(|| {
                AcpError::not_configured(Some(&format!(
                    "agent `{}` (Google Antigravity) command not found: {}",
                    profile.id, profile.command
                )))
            })?;
        (program, profile.args.clone())
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
    if profile.injects_codex_cli_env() && !env.contains_key("CODEX_PATH") {
        if let Some(codex) = find_codex() {
            env.insert("CODEX_PATH".into(), codex.to_string_lossy().to_string());
        }
    }
    augment_spawn_env(profile, &mut env);

    Ok(LaunchSpec {
        program,
        args,
        env,
        display_name: profile.name.clone(),
    })
}

fn resolve_program(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.is_absolute() || command.contains('/') || command.contains('\\') {
        return path.is_file().then(|| path.to_path_buf());
    }
    find_command(command)
}

fn resolve_codex_acp_fallback(profile: &AgentProfile) -> Result<(PathBuf, Vec<String>), AcpError> {
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
        "codex-acp launcher requires bun, bunx, or a standalone codex-acp adapter",
    )))
}

fn augment_spawn_env(profile: &AgentProfile, env: &mut HashMap<String, String>) {
    // Strip PyInstaller environment variables so child processes don't trigger security checks
    env.retain(|k, _| !k.starts_with("_PYI") && !k.starts_with("_MEI"));

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

    if profile.injects_antigravity_proxy_env() {
        let proxy_port = env
            .get("ACP_PROXY_PORT")
            .and_then(|s| s.trim().parse::<u16>().ok())
            .unwrap_or(7897);
        let http_proxy = format!("http://127.0.0.1:{proxy_port}");
        let socks_proxy = format!("socks5://127.0.0.1:{proxy_port}");

        env.entry("HTTP_PROXY".into())
            .or_insert_with(|| http_proxy.clone());
        env.entry("HTTPS_PROXY".into())
            .or_insert_with(|| http_proxy.clone());
        env.entry("http_proxy".into())
            .or_insert_with(|| http_proxy.clone());
        env.entry("https_proxy".into())
            .or_insert_with(|| http_proxy.clone());
        env.entry("ALL_PROXY".into())
            .or_insert_with(|| socks_proxy.clone());
        env.entry("all_proxy".into())
            .or_insert_with(|| socks_proxy.clone());
        env.entry("NO_PROXY".into())
            .or_insert_with(|| "localhost,127.0.0.1,::1".into());
        env.entry("no_proxy".into())
            .or_insert_with(|| "localhost,127.0.0.1,::1".into());
        return;
    }

    if !profile.injects_codex_cli_env() {
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

/// Pick an auth method compatible with the local setup. Auth *policy* lives
/// on the profile (`codex-local` reads Codex config/env, `antigravity-oauth`
/// prefers Google OAuth); profiles without a policy fall back to static
/// `auth_methods` or the first advertised method. `wire` only parses
/// `InitializeResult` and constructs requests.
pub fn pick_auth_method<'a>(
    profile: &AgentProfile,
    init: &'a InitializeResult,
) -> Option<&'a AuthMethod> {
    if init.auth_methods.is_empty() {
        return None;
    }
    if profile.is_antigravity_oauth() {
        if let Some(method) = init.auth_methods.iter().find(|m| m.id == "oauth-personal") {
            return Some(method);
        }
        if let Some(method) = init.auth_methods.iter().find(|m| m.id == "gemini-api-key") {
            if std::env::var("GEMINI_API_KEY").is_ok() {
                return Some(method);
            }
        }
    }
    if profile.is_codex_local_auth() {
        let order: &[&str] = if codex_config_present() {
            &["chat-gpt", "chat-gpt-device-code", "gateway", "api-key"]
        } else if std::env::var("OPENAI_API_KEY").is_ok() {
            &["api-key", "chat-gpt", "chat-gpt-device-code", "gateway"]
        } else {
            &["chat-gpt", "chat-gpt-device-code", "api-key", "gateway"]
        };
        for id in order {
            if let Some(method) = init.auth_methods.iter().find(|method| method.id == *id) {
                return Some(method);
            }
        }
    }
    for id in &profile.auth_methods {
        if let Some(method) = init.auth_methods.iter().find(|method| &method.id == id) {
            return Some(method);
        }
    }
    init.auth_methods.first()
}
