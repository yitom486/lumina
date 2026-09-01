//! Status aggregation for ACP (uses profiles + discovery).

use std::path::{Path, PathBuf};

use crate::acp::discover::{codex_config_present, find_acp_adapter, find_bunx, find_codex};
use crate::acp::error::AcpError;
use crate::acp::model::AcpStatus;
use crate::acp::model::AgentProfilesHint;
use crate::acp::profile::{
    install_hint, list_status, prepare_profiles, AgentKind, RESPONSES_ONLY_NOTE,
};
use crate::process_util::command;

#[cfg(test)]
use crate::acp::profile::default_profiles_hint;

/// Resolve an absolute session `cwd` for ACP.
///
/// Preference: explicit hint (directory, or parent of a media file) → process cwd.
/// Relative hints are joined with the process cwd. ACP requires an absolute path.
pub fn resolve_session_cwd(hint: Option<&str>) -> Result<PathBuf, AcpError> {
    if let Some(raw) = hint.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(raw);
        let absolute = if path.is_absolute() {
            path
        } else {
            let base = std::env::current_dir().map_err(|error| {
                AcpError::internal(Some(&format!("current_dir failed: {error}")))
            })?;
            base.join(path)
        };

        if absolute.is_dir() {
            return Ok(normalize_abs(absolute));
        }
        if let Some(parent) = absolute.parent() {
            if parent.as_os_str().is_empty() {
                return Err(AcpError::bad_request("工作目录无效"));
            }
            if parent.is_dir() || !parent.exists() {
                // Parent of a media file is the workspace even if we cannot verify yet.
                return Ok(normalize_abs(parent.to_path_buf()));
            }
        }
        return Err(AcpError::bad_request("工作目录必须是绝对路径"));
    }

    let cwd = std::env::current_dir()
        .map_err(|error| AcpError::internal(Some(&format!("current_dir failed: {error}"))))?;
    Ok(normalize_abs(cwd))
}

fn normalize_abs(path: PathBuf) -> PathBuf {
    // Best-effort canonicalize; fall back to the absolute path we already have.
    match path.canonicalize() {
        Ok(canonical) => canonical,
        Err(_) => path,
    }
}

#[derive(Debug, Clone)]
pub struct AcpPaths {
    pub cli: std::path::PathBuf,
    pub codex: Option<std::path::PathBuf>,
}

/// Legacy helper / tests: resolve default Codex adapter only.
pub fn resolve_acp_paths() -> Result<AcpPaths, AcpError> {
    let cli = find_acp_adapter().ok_or_else(|| {
        AcpError::not_configured(Some(
            "missing codex-acp on PATH or under src-tauri/native/acp/",
        ))
    })?;
    Ok(AcpPaths {
        cli,
        codex: find_codex(),
    })
}

pub fn status_from_profiles(hint: &AgentProfilesHint) -> AcpStatus {
    let adapter_found = find_acp_adapter().is_some();
    let bunx_found = find_bunx().is_some();
    let codex_found = find_codex().is_some();
    let codex_config_found = codex_config_present();
    let prepared = prepare_profiles(hint);
    let (active_id, profiles) = list_status(&prepared);

    let active = profiles.iter().find(|p| p.id == active_id);
    let available = active.map(|p| p.available).unwrap_or(false)
        && !(active.map(|p| p.command.is_empty()).unwrap_or(true));

    let cli_path = active
        .and_then(|p| p.resolved_command.clone())
        .or_else(|| find_acp_adapter().map(|p| p.to_string_lossy().to_string()));

    let message = if available {
        let name = active.map(|p| p.name.as_str()).unwrap_or("Agent");
        let is_codex = active.map(|p| p.kind == AgentKind::Codex).unwrap_or(false);
        let codex_home = codex_home_label();
        if is_codex && codex_found && codex_config_found {
            format!("{name} 已检测到本机配置，发起提问时将验证连接")
        } else if is_codex && codex_found {
            format!("{name} 已找到，但未检测到 {codex_home} 登录配置")
        } else if is_codex && bunx_found {
            format!("{name} 启动器已找到，首次提问将下载并验证 Agent")
        } else {
            format!("{name} 已就绪（仅在你发起会话时启动）")
        }
    } else if let Some(p) = active {
        if p.kind == AgentKind::Custom && p.command.is_empty() {
            "自定义 Agent 尚未填写启动命令".into()
        } else {
            format!("当前 Agent「{}」不可用：找不到 {}", p.name, p.command)
        }
    } else {
        AcpError::not_configured(None).message
    };

    AcpStatus {
        available,
        adapter_found,
        codex_found,
        codex_config_found,
        active_profile_id: active_id,
        profiles,
        cli_path,
        codex_path: find_codex().map(|p| p.to_string_lossy().to_string()),
        message,
        hint: install_hint(adapter_found, codex_found, bunx_found, codex_config_found),
        responses_only_note: RESPONSES_ONLY_NOTE.into(),
        session_active: false,
        busy: false,
        session_model_options: None,
    }
}

fn codex_home_label() -> &'static str {
    if cfg!(windows) {
        "%USERPROFILE%\\.codex"
    } else {
        "~/.codex"
    }
}

pub fn probe_cli_version(cli: &Path) -> Option<String> {
    let output = command(cli).arg("--version").output().ok()?;
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
        let hint = default_profiles_hint();
        let status = status_from_profiles(&hint);
        assert!(
            status
                .message
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{}",
            status.message
        );
        assert!(
            status
                .hint
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{}",
            status.hint
        );
        assert!(status.responses_only_note.contains("Responses"));
    }

    #[test]
    fn resolve_cwd_uses_process_dir_when_hint_missing() {
        let cwd = resolve_session_cwd(None).expect("cwd");
        assert!(cwd.is_absolute());
    }

    #[test]
    fn resolve_cwd_takes_parent_of_file_hint() {
        let tmp = std::env::temp_dir().join("lumina-acp-cwd-file.mp4");
        let _ = std::fs::write(&tmp, b"x");
        let cwd = resolve_session_cwd(Some(tmp.to_str().expect("utf8"))).expect("cwd");
        assert_eq!(
            cwd,
            tmp.parent()
                .expect("parent")
                .canonicalize()
                .unwrap_or_else(|_| tmp.parent().unwrap().to_path_buf())
        );
        let _ = std::fs::remove_file(&tmp);
    }
}
