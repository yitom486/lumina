//! Filesystem / PATH discovery for ACP agents (no config I/O).

use std::path::{Path, PathBuf};

/// Dev-workspace `native/acp` roots, nearest first. Monorepo 拆分后 acp crate
/// 不再与 `native/` 同目录，向上兼容查找旧布局。
pub fn native_acp_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    roots.push(dir.join("native").join("acp"));
    for _ in 0..6 {
        let Some(parent) = dir.parent().map(PathBuf::from) else {
            break;
        };
        dir = parent;
        roots.push(dir.join("native").join("acp"));
        roots.push(
            dir.join("apps")
                .join("desktop")
                .join("src-tauri")
                .join("native")
                .join("acp"),
        );
    }
    roots
}

pub fn native_acp_dir() -> PathBuf {
    native_acp_dirs()
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("native")
                .join("acp")
        })
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn find_command(name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    for dir in native_acp_dirs() {
        candidates.push(dir.join(name));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
            candidates.push(dir.join("acp").join(name));
        }
    }

    if let Some(from_path) = which(name) {
        candidates.push(from_path);
    }

    if cfg!(windows) && !name.ends_with(".exe") {
        let with_exe = format!("{name}.exe");
        for dir in native_acp_dirs() {
            candidates.push(dir.join(&with_exe));
        }
        if let Some(from_path) = which(&with_exe) {
            candidates.push(from_path);
        }
    }

    candidates.into_iter().find(|p| p.is_file())
}

pub fn find_codex() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("CODEX_PATH") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(path) = find_command(if cfg!(windows) { "codex.exe" } else { "codex" })
        .or_else(|| find_command("codex"))
    {
        return Some(path);
    }

    codex_fallback_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// Whether `~/.codex/config.toml` (or auth) exists — Codex reads this at runtime.
pub fn codex_config_present() -> bool {
    codex_home_dir()
        .map(|home| home.join("config.toml").is_file() || home.join("auth.json").is_file())
        .unwrap_or(false)
}

/// Whether the user already carries stored Codex credentials (`auth.json`).
/// When present the Codex App Server uses them for prompts on its own, so
/// the ACP `authenticate` step must not force a fresh browser login.
pub fn codex_auth_present() -> bool {
    codex_home_dir()
        .map(|home| home.join("auth.json").is_file())
        .unwrap_or(false)
}

pub fn codex_home_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CODEX_HOME") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return Some(path);
        }
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".codex"))
}

fn codex_fallback_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Some(bunx) = find_bunx() {
        if let Some(bin) = bunx.parent() {
            out.push(bin.join(if cfg!(windows) { "codex.exe" } else { "codex" }));
        }
    }

    if let Some(home) = std::env::var_os("USERPROFILE") {
        let home = PathBuf::from(home);
        out.push(home.join(".bun").join("bin").join("codex.exe"));
        out.push(
            home.join("AppData")
                .join("Roaming")
                .join("npm")
                .join("codex.cmd"),
        );
        out.push(
            home.join("AppData")
                .join("Roaming")
                .join("npm")
                .join("codex"),
        );
    }

    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(
            PathBuf::from(local)
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
                .join("codex.exe"),
        );
    }

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        out.push(home.join(".local").join("bin").join("codex"));
        out.push(home.join(".bun").join("bin").join("codex"));
    }

    out
}

pub fn find_acp_adapter() -> Option<PathBuf> {
    find_command(if cfg!(windows) {
        "codex-acp.exe"
    } else {
        "codex-acp"
    })
    .or_else(|| find_command("codex-acp"))
}

/// Bun installs `bunx` alongside Bun. Prefer the explicit override, then PATH,
/// then Bun's conventional per-user install directory on Windows.
pub fn find_bunx() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("BUNX_PATH") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return Some(path);
        }
    }

    let command = if cfg!(windows) { "bunx.exe" } else { "bunx" };
    if let Some(path) = find_command(command).or_else(|| find_command("bunx")) {
        return Some(path);
    }

    #[cfg(windows)]
    {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            let path = PathBuf::from(home)
                .join(".bun")
                .join("bin")
                .join("bunx.exe");
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

pub fn find_bun() -> Option<PathBuf> {
    if let Some(bunx) = find_bunx() {
        if let Some(dir) = bunx.parent() {
            let bun = dir.join(if cfg!(windows) { "bun.exe" } else { "bun" });
            if bun.is_file() {
                return Some(bun);
            }
        }
    }
    find_command(if cfg!(windows) { "bun.exe" } else { "bun" })
}

/// Locate directory for Antigravity settings and credentials (~/.gemini/antigravity-acp).
pub fn antigravity_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".gemini").join("antigravity-acp"))
}

/// Check if Google OAuth credentials or Gemini API key are stored.
pub fn antigravity_credentials_present() -> bool {
    if std::env::var("GEMINI_API_KEY").is_ok() {
        return true;
    }
    let Some(dir) = antigravity_dir() else {
        return false;
    };
    dir.join("acp_token.json").is_file() || dir.join("acp_business_token.json").is_file()
}

/// Locate the official Google agy_acp_server executable.
pub fn find_antigravity() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("AGY_ACP_SERVER_PATH") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return Some(path);
        }
    }

    let exe_name = if cfg!(windows) {
        "agy_acp_server.exe"
    } else {
        "agy_acp_server.par"
    };

    // 1. Zed external agents cache (e.g. %LOCALAPPDATA%\Zed\external_agents\registry\antigravity-acp\v_*\agy_acp_server.exe)
    #[cfg(windows)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let zed_reg = PathBuf::from(local_app_data)
            .join("Zed")
            .join("external_agents")
            .join("registry")
            .join("antigravity-acp");
        if let Ok(entries) = std::fs::read_dir(&zed_reg) {
            for entry in entries.flatten() {
                let candidate = entry.path().join(exe_name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        let zed_reg = PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("zed")
            .join("external_agents")
            .join("registry")
            .join("antigravity-acp");
        if let Ok(entries) = std::fs::read_dir(&zed_reg) {
            for entry in entries.flatten() {
                let candidate = entry.path().join(exe_name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // 2. ~/.gemini/antigravity-acp/runtime/
    if let Some(gemini_dir) = antigravity_dir() {
        let candidate = gemini_dir.join("runtime").join(exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // 3. PATH
    find_command(if cfg!(windows) {
        "agy_acp_server.exe"
    } else {
        "agy_acp_server"
    })
    .or_else(|| find_command("agy_acp_server"))
}

/// Cursor 登录态候选落盘位置（按优先级排序）。
/// 对齐参考实现：`$XDG_CONFIG_HOME/cursor/auth.json` 优先；Windows 再查
/// `%USERPROFILE%\.config\cursor\auth.json` 与旧 App 落盘 hint
/// `%APPDATA%\Cursor\User\globalStorage\storage.json`。
pub fn cursor_auth_candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |path: PathBuf| {
        if seen.insert(path.clone()) {
            out.push(path);
        }
    };
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let trimmed = xdg.trim();
        if !trimmed.is_empty() {
            push(PathBuf::from(trimmed).join("cursor").join("auth.json"));
        }
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    if cfg!(windows) {
        if let Some(profile) = home.as_ref().map(PathBuf::from) {
            push(profile.join(".config").join("cursor").join("auth.json"));
        }
        if let Ok(roaming) = std::env::var("APPDATA") {
            let trimmed = roaming.trim();
            if !trimmed.is_empty() {
                push(
                    PathBuf::from(trimmed)
                        .join("Cursor")
                        .join("User")
                        .join("globalStorage")
                        .join("storage.json"),
                );
            }
        }
    } else if let Some(home) = home.map(PathBuf::from) {
        push(home.join(".config").join("cursor").join("auth.json"));
    }
    out
}

fn is_cursor_storage_hint(path: &Path) -> bool {
    path.to_string_lossy()
        .replace('\\', "/")
        .ends_with("/globalStorage/storage.json")
}

/// `auth.json` 有效性：非空可解析且顶层 `accessToken` / `refreshToken` 任一非空。
/// `storage.json` 只做 hint 级判定（存在且为非空对象）。
fn is_valid_cursor_auth_file(path: &PathBuf) -> bool {
    if is_cursor_storage_hint(path) {
        return false;
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    if raw.trim().is_empty() {
        return false;
    }
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let Some(row) = parsed.as_object() else {
        return false;
    };
    let non_empty = |key: &str| {
        row.get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    non_empty("accessToken") || non_empty("refreshToken")
}

/// Whether the user already carries stored Cursor credentials.
/// Hint-level only（新版 token 可能进 OS keychain，文件未命中不断言未登录）：
/// `CURSOR_API_KEY` / `CURSOR_AUTH_TOKEN` 环境透传（子进程继承父环境）或
/// 任一候选 auth.json 有效即算命中。命中时 ACP `authenticate` 必须跳过，
/// 由 agent 自己用本机登录态建会话。
pub fn cursor_auth_present() -> bool {
    if std::env::var("CURSOR_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    if std::env::var("CURSOR_AUTH_TOKEN")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    cursor_auth_candidates()
        .iter()
        .any(is_valid_cursor_auth_file)
}

/// Locate the official Cursor CLI (`agent acp`).
/// 1. 官方安装位直查（Windows `%LOCALAPPDATA%\cursor-agent\agent.cmd`，
///    posix `~/.local/bin/agent`）；2. PATH。皆无返回 None（未安装），
///    由调用方转成带安装指引的 `NotConfigured`，不把裸命令丢给 spawn。
pub fn find_cursor_agent() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("CURSOR_AGENT_PATH") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return Some(path);
        }
    }

    if cfg!(windows) {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let candidate = PathBuf::from(local_app_data)
                .join("cursor-agent")
                .join("agent.cmd");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    } else if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join(".local").join("bin").join("agent");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let binary = if cfg!(windows) { "agent.cmd" } else { "agent" };
    find_command(binary)
}

/// Dev tree: `node_modules/@agentclientprotocol/codex-acp` (avoids flaky `bun x` on Windows).
/// Monorepo 拆分后向上兼容查找，desktop 包优先（旧解析顺序）。
pub fn find_dev_codex_acp_entry() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        for node_modules in [
            dir.join("apps").join("desktop").join("node_modules"),
            dir.join("node_modules"),
        ] {
            let entry = node_modules
                .join("@agentclientprotocol")
                .join("codex-acp")
                .join("dist")
                .join("index.js");
            if entry.is_file() {
                return Some(entry.canonicalize().unwrap_or(entry));
            }
        }
        let Some(parent) = dir.parent().map(PathBuf::from) else {
            break;
        };
        dir = parent;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_helpers_do_not_panic() {
        let _ = codex_config_present();
        let _ = codex_auth_present();
        let _ = codex_home_dir();
        let _ = codex_fallback_candidates();
        let _ = antigravity_dir();
        let _ = antigravity_credentials_present();
        let _ = find_antigravity();
        let _ = cursor_auth_candidates();
        let _ = cursor_auth_present();
        let _ = find_cursor_agent();
    }

    #[test]
    fn cursor_auth_candidates_cover_xdg_and_home() {
        let candidates = cursor_auth_candidates();
        assert!(!candidates.is_empty());
        assert!(candidates
            .iter()
            .any(|path| path.ends_with(PathBuf::from("cursor").join("auth.json"))));
    }

    #[test]
    fn cursor_auth_file_requires_a_token() {
        let dir = std::env::temp_dir().join("lumina-cursor-auth-probe");
        let _ = std::fs::create_dir_all(&dir);
        let valid = dir.join("auth.json");
        let _ = std::fs::write(&valid, r#"{"accessToken":"  abc  "}"#);
        assert!(is_valid_cursor_auth_file(&valid));
        let empty = dir.join("empty.json");
        let _ = std::fs::write(&empty, r#"{"accessToken":"  "}"#);
        assert!(!is_valid_cursor_auth_file(&empty));
        let dirty = dir.join("dirty.json");
        let _ = std::fs::write(&dirty, "not-json");
        assert!(!is_valid_cursor_auth_file(&dirty));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
