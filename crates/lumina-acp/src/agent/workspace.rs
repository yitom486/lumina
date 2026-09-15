//! ACP session workspace (cwd resolution + legacy paths).

use std::path::PathBuf;

use crate::agent::discover::{find_acp_adapter, find_codex};
use crate::error::AcpError;

/// Resolve an absolute session `cwd` for ACP.
///
/// Preference: explicit hint (directory, or parent of a media file) → process cwd.
/// Relative hints are joined with the process cwd. ACP requires an absolute path.
pub fn resolve_session_cwd(hint: Option<&str>) -> Result<PathBuf, AcpError> {
    if let Some(raw) = hint.map(str::trim).filter(|s| !s.is_empty()) {
        if raw.to_ascii_lowercase().starts_with("http://")
            || raw.to_ascii_lowercase().starts_with("https://")
        {
            tracing::warn!(cwd_hint = raw, "rejected remote URL as ACP workspace");
            return Err(AcpError::workspace_unavailable(Some(
                "remote URL cannot be used as ACP cwd",
            )));
        }
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
        if absolute.is_file() {
            if let Some(parent) = absolute.parent().filter(|parent| parent.is_dir()) {
                return Ok(normalize_abs(parent.to_path_buf()));
            }
        }
        tracing::warn!(cwd = %absolute.display(), "ACP workspace is not an existing directory");
        return Err(AcpError::workspace_unavailable(Some(&format!(
            "workspace does not exist or is not a directory: {}",
            absolute.display()
        ))));
    }

    let workspace = default_session_cwd();
    std::fs::create_dir_all(&workspace).map_err(|error| {
        tracing::warn!(cwd = %workspace.display(), %error, "failed to create default ACP workspace");
        AcpError::workspace_unavailable(Some(&format!(
            "create default workspace {}: {error}",
            workspace.display()
        )))
    })?;
    Ok(normalize_abs(workspace))
}

fn default_session_cwd() -> PathBuf {
    #[cfg(windows)]
    if let Some(base) = std::env::var_os("APPDATA") {
        return PathBuf::from(base).join("lumina").join("acp-workspace");
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("lumina")
            .join("acp-workspace");
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(base).join("lumina").join("acp-workspace");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("lumina")
                .join("acp-workspace");
        }
    }

    std::env::temp_dir().join("lumina").join("acp-workspace")
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

/// Legacy helper: resolve default Codex adapter only; prefer profile launch resolution.
#[deprecated(note = "legacy helper for default Codex adapter only; use profile launch resolution")]
pub fn resolve_acp_paths() -> Result<AcpPaths, AcpError> {
    let cli = find_acp_adapter().ok_or_else(|| {
        AcpError::not_configured(Some(
            "missing codex-acp on PATH or under apps/desktop/src-tauri/native/acp/",
        ))
    })?;
    Ok(AcpPaths {
        cli,
        codex: find_codex(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cwd_uses_writable_app_workspace_when_hint_missing() {
        let cwd = resolve_session_cwd(None).expect("cwd");
        assert!(cwd.is_absolute());
        assert!(cwd.is_dir());
        assert!(cwd.ends_with("acp-workspace"));
    }

    #[test]
    fn resolve_cwd_rejects_remote_urls_with_business_error() {
        let error = resolve_session_cwd(Some("https://www.youtube.com/watch?v=test"))
            .expect_err("URL must not become cwd");
        assert_eq!(error.code, crate::AcpErrorCode::WorkspaceUnavailable);
        assert!(error.message.contains("工作目录"));
        assert!(!error.message.contains("https"));
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
