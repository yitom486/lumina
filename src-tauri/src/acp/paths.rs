//! Resolve optional on-demand codex-acp (+ optional codex). Never started at app boot.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::acp::error::AcpError;
use crate::acp::model::AcpStatus;

#[derive(Debug, Clone)]
pub struct AcpPaths {
    pub cli: PathBuf,
    pub codex: Option<PathBuf>,
}

pub fn resolve_acp_paths() -> Result<AcpPaths, AcpError> {
    let cli = find_acp_cli().ok_or_else(|| {
        AcpError::not_configured(Some(
            "missing codex-acp on PATH or under src-tauri/native/acp/",
        ))
    })?;
    let codex = find_codex();
    Ok(AcpPaths { cli, codex })
}

pub fn status() -> AcpStatus {
    match resolve_acp_paths() {
        Ok(paths) => AcpStatus {
            available: true,
            cli_path: Some(paths.cli.to_string_lossy().to_string()),
            codex_path: paths.codex.map(|p| p.to_string_lossy().to_string()),
            message: "ACP 已就绪（仅在你发起会话时启动）".into(),
        },
        Err(error) => AcpStatus {
            available: false,
            cli_path: None,
            codex_path: None,
            message: error.message,
        },
    }
}

fn find_acp_cli() -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("native")
            .join("acp")
            .join(if cfg!(windows) {
                "codex-acp.exe"
            } else {
                "codex-acp"
            }),
    ];

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(if cfg!(windows) {
                "codex-acp.exe"
            } else {
                "codex-acp"
            }));
            candidates.push(dir.join("acp").join(if cfg!(windows) {
                "codex-acp.exe"
            } else {
                "codex-acp"
            }));
        }
    }

    if let Some(from_path) = which("codex-acp") {
        candidates.push(from_path);
    }
    if cfg!(windows) {
        if let Some(from_path) = which("codex-acp.exe") {
            candidates.push(from_path);
        }
    }

    candidates.into_iter().find(|p| p.is_file())
}

fn find_codex() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(override_path) = std::env::var("CODEX_PATH") {
        let p = PathBuf::from(override_path);
        if p.is_file() {
            return Some(p);
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("native")
            .join("acp")
            .join(if cfg!(windows) { "codex.exe" } else { "codex" }),
    );
    if let Some(p) = which("codex") {
        candidates.push(p);
    }
    if cfg!(windows) {
        if let Some(p) = which("codex.exe") {
            candidates.push(p);
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Best-effort PATH lookup without extra crates.
fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn probe_cli_version(cli: &Path) -> Option<String> {
    let output = Command::new(cli).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_message_is_chinese_when_missing() {
        // On CI without codex-acp this is the common path.
        let status = status();
        assert!(
            status.message.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{}",
            status.message
        );
    }
}
