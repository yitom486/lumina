//! On-disk snapshot written before each chat prompt; MCP tools read this file.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::library::SeriesLibraryCache;
use crate::library::{lumina_agent_context_path, lumina_tmp_dir};

/// Relative path under session cwd; must stay aligned with [`lumina_agent_context_path`].
pub const SNAPSHOT_RELATIVE_PATH: &str = ".lumina/agent-context.json";
pub const CONTEXT_FILE_ENV: &str = "LUMINA_MCP_CONTEXT_FILE";
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 2;
pub const LIBRARY_WARM_EVERY: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptAnchor {
    pub media_path: String,
    pub library_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
    pub position_ms: u64,
    pub sent_at_ms: u128,
    pub subtitle_choice_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackLite {
    pub media_path: Option<String>,
    pub media_title: Option<String>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub chapter_title: Option<String>,
    pub notes_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPolicy {
    pub turn: u32,
    pub media_path: Option<String>,
    pub library_warmed_turn: u32,
    pub library_warm_every: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub vision_capable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LuminaMcpSnapshot {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub anchor: Option<PromptAnchor>,
    pub playback: Option<PlaybackLite>,
    pub library: Option<SeriesLibraryCache>,
    pub session: Option<SessionPolicy>,
    pub capabilities: Option<AgentCapabilities>,
    pub updated_at_ms: u128,
}

fn default_schema_version() -> u32 {
    SNAPSHOT_SCHEMA_VERSION
}

impl LuminaMcpSnapshot {
    pub fn empty() -> Self {
        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: None,
            playback: None,
            library: None,
            session: None,
            capabilities: None,
            updated_at_ms: now_ms(),
        }
    }
}

pub fn should_warm_series_library(turn: u32, path_changed: bool) -> bool {
    path_changed || turn == 1 || turn % LIBRARY_WARM_EVERY == 0
}

pub fn snapshot_path_for_cwd(cwd: &Path) -> PathBuf {
    lumina_agent_context_path(cwd)
}

pub fn ephemeral_tmp_dir(cwd: &Path) -> PathBuf {
    lumina_tmp_dir(cwd)
}

pub fn cleanup_ephemeral_tmp(cwd: &Path) {
    let tmp = ephemeral_tmp_dir(cwd);
    let _ = fs::remove_dir_all(tmp);
}

pub fn write_snapshot(path: &Path, snapshot: &LuminaMcpSnapshot) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create snapshot dir: {error}"))?;
    }
    if let Some(cwd) = path.parent().and_then(|p| p.parent()) {
        cleanup_ephemeral_tmp(cwd);
    }
    let payload =
        serde_json::to_string_pretty(snapshot).map_err(|error| format!("encode snapshot: {error}"))?;
    fs::write(path, payload).map_err(|error| format!("write snapshot: {error}"))
}

pub fn read_snapshot(path: &Path) -> Result<LuminaMcpSnapshot, String> {
    let raw = fs::read_to_string(path).map_err(|error| format!("read snapshot: {error}"))?;
    serde_json::from_str(&raw).map_err(|error| format!("parse snapshot: {error}"))
}

pub fn sync_snapshot_capabilities(path: &Path, vision_capable: bool) -> Result<(), String> {
    let mut snapshot = if path.is_file() {
        read_snapshot(path).unwrap_or_else(|_| LuminaMcpSnapshot::empty())
    } else {
        LuminaMcpSnapshot::empty()
    };
    snapshot.capabilities = Some(AgentCapabilities {
        vision_capable,
    });
    snapshot.updated_at_ms = now_ms();
    write_snapshot(path, &snapshot)
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
    fn warm_policy_matches_turn_one_and_every_fifth() {
        assert!(should_warm_series_library(1, false));
        assert!(!should_warm_series_library(2, false));
        assert!(!should_warm_series_library(4, false));
        assert!(should_warm_series_library(5, false));
        assert!(!should_warm_series_library(6, false));
        assert!(should_warm_series_library(3, true));
    }

    #[test]
    fn roundtrip_snapshot_json() {
        let dir = std::env::temp_dir().join(format!("lumina-mcp-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("agent-context.json");
        let snapshot = LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: Some(PromptAnchor {
                media_path: r"D:\videos\demo.mkv".into(),
                library_root: Some(r"D:\library".into()),
                group_key: Some("Demo.Show".into()),
                season: Some(1),
                episode: Some(1),
                position_ms: 12_000,
                sent_at_ms: 1,
                subtitle_choice_id: Some("embedded:2".into()),
            }),
            playback: Some(PlaybackLite {
                media_path: Some(r"D:\videos\demo.mkv".into()),
                media_title: Some("demo.mkv".into()),
                position_ms: Some(12_000),
                duration_ms: Some(60_000),
                chapter_title: None,
                notes_excerpt: None,
            }),
            library: Some(SeriesLibraryCache {
                title: "示例".into(),
                synopsis: Some("简介".into()),
                characters: None,
                creators: vec![],
                network: None,
                status: None,
                wiki_attribution: None,
                wiki_page_url: None,
            }),
            session: Some(SessionPolicy {
                turn: 1,
                media_path: Some(r"D:\videos\demo.mkv".into()),
                library_warmed_turn: 1,
                library_warm_every: LIBRARY_WARM_EVERY,
            }),
            capabilities: Some(AgentCapabilities {
                vision_capable: true,
            }),
            updated_at_ms: 1,
        };
        write_snapshot(&path, &snapshot).expect("write");
        let loaded = read_snapshot(&path).expect("read");
        assert_eq!(loaded, snapshot);
        let _ = fs::remove_dir_all(dir);
    }
}
