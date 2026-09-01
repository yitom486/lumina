//! On-disk snapshot written before each chat prompt; MCP tools read this file.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::acp::VideoPromptContext;
use crate::library::MediaMetadataContext;

pub const SNAPSHOT_RELATIVE_PATH: &str = ".lumina/agent-context.json";
pub const CONTEXT_FILE_ENV: &str = "LUMINA_MCP_CONTEXT_FILE";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuminaMcpSnapshot {
    pub playback: Option<VideoPromptContext>,
    pub library: Option<MediaMetadataContext>,
    pub updated_at_ms: u128,
}

impl LuminaMcpSnapshot {
    pub fn new(
        playback: Option<VideoPromptContext>,
        library: Option<MediaMetadataContext>,
    ) -> Self {
        Self {
            playback,
            library,
            updated_at_ms: now_ms(),
        }
    }
}

pub fn snapshot_path_for_cwd(cwd: &Path) -> PathBuf {
    cwd.join(SNAPSHOT_RELATIVE_PATH)
}

pub fn write_snapshot(path: &Path, snapshot: &LuminaMcpSnapshot) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create snapshot dir: {error}"))?;
    }
    let payload =
        serde_json::to_string_pretty(snapshot).map_err(|error| format!("encode snapshot: {error}"))?;
    fs::write(path, payload).map_err(|error| format!("write snapshot: {error}"))
}

pub fn read_snapshot(path: &Path) -> Result<LuminaMcpSnapshot, String> {
    let raw = fs::read_to_string(path).map_err(|error| format!("read snapshot: {error}"))?;
    serde_json::from_str(&raw).map_err(|error| format!("parse snapshot: {error}"))
}

pub fn resolve_snapshot_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var(CONTEXT_FILE_ENV) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    std::env::current_dir()
        .ok()
        .map(|cwd| snapshot_path_for_cwd(&cwd))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_snapshot_json() {
        let dir = std::env::temp_dir().join(format!("lumina-mcp-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("agent-context.json");
        let snapshot = LuminaMcpSnapshot::new(
            Some(VideoPromptContext {
                media_path: Some(r"D:\videos\demo.mp4".into()),
                media_title: Some("demo.mp4".into()),
                ..Default::default()
            }),
            None,
        );
        write_snapshot(&path, &snapshot).expect("write");
        let loaded = read_snapshot(&path).expect("read");
        assert_eq!(loaded.playback, snapshot.playback);
        let _ = fs::remove_dir_all(dir);
    }
}
