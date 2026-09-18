//! Filesystem / PATH discovery for ACP agents (no config I/O).

use std::path::PathBuf;

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
        let _ = codex_home_dir();
        let _ = codex_fallback_candidates();
        let _ = antigravity_dir();
        let _ = antigravity_credentials_present();
        let _ = find_antigravity();
    }
}
