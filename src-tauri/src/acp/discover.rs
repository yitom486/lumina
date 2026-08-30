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
        let p = PathBuf::from(override_path);
        if p.is_file() {
            return Some(p);
        }
    }
    find_command(if cfg!(windows) { "codex.exe" } else { "codex" })
        .or_else(|| find_command("codex"))
}

pub fn find_acp_adapter() -> Option<PathBuf> {
    find_command(if cfg!(windows) {
        "codex-acp.exe"
    } else {
        "codex-acp"
    })
    .or_else(|| find_command("codex-acp"))
}
