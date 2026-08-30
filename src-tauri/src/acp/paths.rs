//! Status aggregation for ACP (uses profiles + discovery).

use std::path::Path;
use std::process::Command;

use crate::acp::discover::{find_acp_adapter, find_codex};
use crate::acp::error::AcpError;
use crate::acp::model::AcpStatus;
use crate::acp::profile::{
    install_hint, AgentKind, ProfileStore, RESPONSES_ONLY_NOTE,
};

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

pub fn status_from_store(store: &ProfileStore) -> AcpStatus {
    let adapter_found = find_acp_adapter().is_some();
    let codex_found = find_codex().is_some();
    let (active_id, profiles) = match store.list_status() {
        Ok(v) => v,
        Err(error) => {
            return AcpStatus {
                available: false,
                adapter_found,
                codex_found,
                active_profile_id: "codex".into(),
                profiles: Vec::new(),
                cli_path: None,
                codex_path: find_codex().map(|p| p.to_string_lossy().to_string()),
                message: error.message,
                hint: install_hint(adapter_found, codex_found),
                responses_only_note: RESPONSES_ONLY_NOTE.into(),
            };
        }
    };

    let active = profiles.iter().find(|p| p.id == active_id);
    let available = active.map(|p| p.available).unwrap_or(false)
        && !(active.map(|p| p.command.is_empty()).unwrap_or(true));

    let cli_path = active
        .and_then(|p| p.resolved_command.clone())
        .or_else(|| find_acp_adapter().map(|p| p.to_string_lossy().to_string()));

    let message = if available {
        let name = active.map(|p| p.name.as_str()).unwrap_or("Agent");
        format!("{name} 已就绪（仅在你发起会话时启动）")
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
        active_profile_id: active_id,
        profiles,
        cli_path,
        codex_path: find_codex().map(|p| p.to_string_lossy().to_string()),
        message,
        hint: install_hint(adapter_found, codex_found),
        responses_only_note: RESPONSES_ONLY_NOTE.into(),
    }
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
        let store = ProfileStore::new();
        let status = status_from_store(&store);
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
}
