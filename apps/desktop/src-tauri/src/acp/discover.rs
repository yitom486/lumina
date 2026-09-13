//! Filesystem / PATH discovery for ACP agents (no config I/O).

use std::path::PathBuf;

pub fn native_acp_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("native")
        .join("acp")
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

    candidates.push(native_acp_dir().join(name));

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
        candidates.push(native_acp_dir().join(&with_exe));
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

/// Dev tree: `node_modules/@agentclientprotocol/codex-acp` (avoids flaky `bun x` on Windows).
pub fn find_dev_codex_acp_entry() -> Option<PathBuf> {
    let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("node_modules")
        .join("@agentclientprotocol")
        .join("codex-acp")
        .join("dist")
        .join("index.js");
    if !entry.is_file() {
        return None;
    }
    Some(entry.canonicalize().unwrap_or(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_helpers_do_not_panic() {
        let _ = codex_config_present();
        let _ = codex_home_dir();
        let _ = codex_fallback_candidates();
    }
}
