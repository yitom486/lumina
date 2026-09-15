//! Filesystem half of `AcpHost`: read/write/resolve against session workspace.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::error::AcpError;

use super::AcpHost;

impl AcpHost {
    pub(super) fn read_text_file(&self, params: &Value) -> Result<Value, AcpError> {
        let path = self.resolve_path(params, "path")?;
        let line = params
            .get("line")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1) as usize;
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);

        let raw = fs::read_to_string(&path).map_err(|error| {
            tracing::warn!(path = %path.display(), %error, "ACP fs read failed");
            AcpError::new(
                crate::AcpErrorCode::ProtocolError,
                "无法读取该文件",
                Some(error.to_string()),
            )
        })?;

        let content = if line <= 1 && limit.is_none() {
            raw
        } else {
            let lines: Vec<&str> = raw.lines().collect();
            let start = line.saturating_sub(1).min(lines.len());
            let end = match limit {
                Some(n) => (start + n).min(lines.len()),
                None => lines.len(),
            };
            lines[start..end].join("\n")
        };

        tracing::info!(path = %path.display(), bytes = content.len(), "ACP fs/read_text_file");
        Ok(json!({ "content": content }))
    }

    pub(super) fn write_text_file(&self, params: &Value) -> Result<(), AcpError> {
        let path = self.resolve_path(params, "path")?;
        let content = params
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| AcpError::bad_request("写入内容缺失"))?;

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|error| {
                    tracing::warn!(path = %parent.display(), %error, "ACP fs mkdir failed");
                    AcpError::new(
                        crate::AcpErrorCode::ProtocolError,
                        "无法创建文件目录",
                        Some(error.to_string()),
                    )
                })?;
            }
        }

        fs::write(&path, content).map_err(|error| {
            tracing::warn!(path = %path.display(), %error, "ACP fs write failed");
            AcpError::new(
                crate::AcpErrorCode::ProtocolError,
                "无法写入该文件",
                Some(error.to_string()),
            )
        })?;

        tracing::info!(path = %path.display(), bytes = content.len(), "ACP fs/write_text_file");
        Ok(())
    }

    pub(super) fn workspace_cwd(&self) -> Result<PathBuf, AcpError> {
        self.workspace
            .lock()
            .map_err(|_| AcpError::internal(Some("workspace lock poisoned")))?
            .clone()
            .ok_or_else(|| AcpError::protocol(Some("session workspace cwd missing")))
    }

    /// Absolute paths preferred; relative paths resolve against session workspace.
    pub(super) fn resolve_path(&self, params: &Value, key: &str) -> Result<PathBuf, AcpError> {
        let raw = require_str(params, key)?;
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            return Ok(path);
        }
        let base = self.workspace_cwd()?;
        Ok(base.join(path))
    }
}

pub(super) fn require_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, AcpError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| AcpError::bad_request(format!("缺少参数 {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("lumina-acp-{nanos}-{name}"))
    }

    #[test]
    fn read_write_roundtrip() {
        let host = AcpHost::new();
        let path = temp_path("sample.txt");
        fs::write(&path, "a\nb\nc").expect("seed");

        let read = host
            .read_text_file(&json!({
                "path": path.to_string_lossy(),
                "line": 2,
                "limit": 1
            }))
            .expect("read");
        assert_eq!(read.get("content").and_then(Value::as_str), Some("b"));

        let path2 = temp_path("out.txt");
        host.write_text_file(&json!({
            "path": path2.to_string_lossy(),
            "content": "hello"
        }))
        .expect("write");
        assert_eq!(fs::read_to_string(&path2).expect("reread"), "hello");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&path2);
    }

    #[test]
    fn relative_path_resolves_against_workspace() {
        let host = AcpHost::new();
        let dir = temp_path("ws");
        fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("note.txt");
        fs::write(&file, "workspace-rel").expect("seed");
        host.set_workspace(dir.clone());

        let read = host
            .read_text_file(&json!({ "path": "note.txt" }))
            .expect("read relative");
        assert_eq!(
            read.get("content").and_then(Value::as_str),
            Some("workspace-rel")
        );
        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_relative_without_workspace() {
        let host = AcpHost::new();
        let err = host
            .read_text_file(&json!({ "path": "relative.txt" }))
            .expect_err("relative");
        assert_eq!(err.message, "与 Agent 通信失败");
    }
}
