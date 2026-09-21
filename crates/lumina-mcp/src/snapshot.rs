//! On-disk snapshot written before each chat prompt; MCP tools read this file.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use lumina_library::SeriesLibraryCache;
use lumina_library::{lumina_agent_context_path, lumina_tmp_dir};
use lumina_media::MediaChapter;
use lumina_subtitle::{SubtitleChoice, Transcript};

/// Relative path under session cwd; must stay aligned with [`lumina_agent_context_path`].
pub const SNAPSHOT_RELATIVE_PATH: &str = ".lumina/agent-context.json";
pub const CONTEXT_FILE_ENV: &str = "LUMINA_MCP_CONTEXT_FILE";
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 5;
pub const LIBRARY_WARM_EVERY: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptAnchor {
    pub media_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_title: Option<String>,
    pub library_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
    pub position_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub sent_at_ms: u128,
    pub subtitle_choice_id: Option<String>,
    /// Preferred default radius for the current chat transcript tool call.
    /// Explicit MCP arguments always take precedence; omitted values retain
    /// the historical 60-second default when this field is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_window_radius_sec: Option<u64>,
}

/// Current-episode plot kept on disk for MCP; also inlined into the prompt
/// **once** when the media/episode changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CurrentEpisodeLite {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
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
    /// When false (default for main chat), subtitle workshop MCP tools are hidden.
    #[serde(default)]
    pub subtitle_workshop_enabled: bool,
    /// When true (default for main chat), Agent may propose video annotations.
    #[serde(default = "default_video_annotations_enabled")]
    pub video_annotations_enabled: bool,
}

/// Task-scoped context for the Chapter Agent. It is serialized as the
/// optional `chapterTask` sidecar field so existing chat snapshot literals and
/// readers remain source-compatible while chapter sessions gain a strict
/// database/media scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChapterTaskContext {
    pub task_id: i64,
    pub attempt_id: i64,
    pub episode_id: i64,
    pub database_path: String,
    pub media_path: String,
    pub duration_ms: u64,
    pub spoiler_boundary: String,
    pub prompt_version: String,
}

fn default_video_annotations_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OnlineMediaSnapshot {
    pub media_id: String,
    pub title: Option<String>,
    pub duration_ms: Option<u64>,
    pub webpage_url: Option<String>,
    pub extractor: Option<String>,
    pub chapters: Vec<MediaChapter>,
    pub subtitles: Vec<SubtitleChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<Transcript>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LuminaMcpSnapshot {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub anchor: Option<PromptAnchor>,
    /// Per-episode title/overview for the anchored media (not the series wiki).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_episode: Option<CurrentEpisodeLite>,
    pub library: Option<SeriesLibraryCache>,
    pub session: Option<SessionPolicy>,
    pub capabilities: Option<AgentCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online: Option<OnlineMediaSnapshot>,
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
            current_episode: None,
            library: None,
            session: None,
            capabilities: None,
            online: None,
            updated_at_ms: now_ms(),
        }
    }
}

pub fn should_warm_series_library(turn: u32, path_changed: bool) -> bool {
    path_changed || turn == 1 || turn.is_multiple_of(LIBRARY_WARM_EVERY)
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
    let payload = serde_json::to_string_pretty(snapshot)
        .map_err(|error| format!("encode snapshot: {error}"))?;
    fs::write(path, payload).map_err(|error| format!("write snapshot: {error}"))
}

pub fn write_chapter_task_snapshot(
    path: &Path,
    snapshot: &LuminaMcpSnapshot,
    chapter_task: &ChapterTaskContext,
) -> Result<(), String> {
    let mut value =
        serde_json::to_value(snapshot).map_err(|error| format!("encode snapshot: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "snapshot must encode as an object".to_string())?;
    object.insert(
        "chapterTask".to_string(),
        serde_json::to_value(chapter_task)
            .map_err(|error| format!("encode chapter task: {error}"))?,
    );
    let payload = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("encode chapter task snapshot: {error}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create snapshot dir: {error}"))?;
    }
    if let Some(cwd) = path.parent().and_then(|p| p.parent()) {
        cleanup_ephemeral_tmp(cwd);
    }
    fs::write(path, payload).map_err(|error| format!("write chapter task snapshot: {error}"))
}

pub fn read_chapter_task_context(path: &Path) -> Result<Option<ChapterTaskContext>, String> {
    let raw = fs::read_to_string(path).map_err(|error| format!("read snapshot: {error}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("parse snapshot: {error}"))?;
    let Some(chapter_task) = value.get("chapterTask") else {
        return Ok(None);
    };
    serde_json::from_value(chapter_task.clone())
        .map(Some)
        .map_err(|error| format!("parse chapter task snapshot: {error}"))
}

pub fn read_snapshot(path: &Path) -> Result<LuminaMcpSnapshot, String> {
    let raw = fs::read_to_string(path).map_err(|error| format!("read snapshot: {error}"))?;
    serde_json::from_str(&raw).map_err(|error| format!("parse snapshot: {error}"))
}

fn is_chapter_snapshot_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "chapter-agent-context.json")
}

pub fn sync_snapshot_capabilities(path: &Path, vision_capable: bool) -> Result<(), String> {
    // Chapter scope is intentionally a sidecar so existing chat snapshot
    // literals remain compatible. Preserve it only on the dedicated chapter
    // path; a normal chat snapshot must never inherit stale task scope.
    let chapter_task = if is_chapter_snapshot_path(path) && path.is_file() {
        read_chapter_task_context(path)?
    } else {
        None
    };
    let mut snapshot = if path.is_file() {
        read_snapshot(path).unwrap_or_else(|_| LuminaMcpSnapshot::empty())
    } else {
        LuminaMcpSnapshot::empty()
    };
    snapshot.capabilities = Some(AgentCapabilities {
        vision_capable,
        subtitle_workshop_enabled: false,
        video_annotations_enabled: true,
    });
    snapshot.updated_at_ms = now_ms();
    match chapter_task {
        Some(chapter_task) => write_chapter_task_snapshot(path, &snapshot, &chapter_task),
        None => write_snapshot(path, &snapshot),
    }
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
    fn snapshot_carries_no_cookie_material() {
        // H-P2-7 contract: login state (browser cookies / cookies.txt / mpv-cookies.txt)
        // must never reach the Agent. Tripwire: any future field containing it fails here.
        let snapshot = LuminaMcpSnapshot {
            online: Some(OnlineMediaSnapshot {
                media_id: "youtube:e2e".into(),
                title: Some("demo".into()),
                duration_ms: Some(60_000),
                webpage_url: Some("https://www.youtube.com/watch?v=e2e".into()),
                extractor: Some("youtube".into()),
                chapters: vec![],
                subtitles: vec![SubtitleChoice {
                    id: "online:en".into(),
                    source: lumina_subtitle::SubtitleSource::Sidecar,
                    label: "English".into(),
                    supported: true,
                    stream_index: None,
                    external_path: None,
                    codec_name: Some("srt".into()),
                    language: Some("en".into()),
                }],
                transcript: None,
            }),
            ..LuminaMcpSnapshot::empty()
        };
        let json = serde_json::to_value(&snapshot).expect("snapshot serializes");
        let text = json.to_string().to_lowercase();
        assert!(
            !text.contains("cookie"),
            "agent snapshot must not carry cookie material: {text}"
        );
    }

    #[test]
    fn online_snapshot_hides_cache_paths_and_signed_urls() {
        use lumina_media::MediaChapter;
        use lumina_subtitle::Transcript;
        let snapshot = LuminaMcpSnapshot {
            anchor: Some(PromptAnchor {
                media_path: "https://www.youtube.com/watch?v=e2e".into(),
                media_title: None,
                library_root: None,
                group_key: None,
                season: None,
                episode: None,
                position_ms: 10_000,
                duration_ms: None,
                sent_at_ms: 1,
                subtitle_choice_id: Some("online:en".into()),
                transcript_window_radius_sec: None,
            }),
            online: Some(OnlineMediaSnapshot {
                media_id: "youtube:e2e".into(),
                title: Some("demo".into()),
                duration_ms: Some(60_000),
                webpage_url: Some("https://www.youtube.com/watch?v=e2e".into()),
                extractor: Some("youtube".into()),
                chapters: vec![MediaChapter {
                    id: 0,
                    start_ms: 0,
                    end_ms: Some(10_000),
                    title: Some("Intro".into()),
                }],
                subtitles: vec![SubtitleChoice {
                    id: "online:en".into(),
                    source: lumina_subtitle::SubtitleSource::Sidecar,
                    label: "在线 · en".into(),
                    supported: true,
                    stream_index: None,
                    external_path: None,
                    codec_name: Some("vtt".into()),
                    language: Some("en".into()),
                }],
                transcript: Some(Transcript {
                    source_path: "https://www.youtube.com/watch?v=e2e".into(),
                    choice_id: "online:en".into(),
                    stream_index: None,
                    language: Some("en".into()),
                    codec_name: Some("vtt".into()),
                    cues: vec![lumina_subtitle::Cue {
                        index: 1,
                        start_ms: 9_000,
                        end_ms: 11_000,
                        text: "hello".into(),
                    }],
                }),
            }),
            capabilities: Some(AgentCapabilities {
                vision_capable: false,
                subtitle_workshop_enabled: false,
                video_annotations_enabled: true,
            }),
            ..LuminaMcpSnapshot::empty()
        };
        let json = serde_json::to_value(&snapshot).expect("snapshot serializes");
        let text = json.to_string().to_lowercase();
        for banned in [
            "cookie",
            "sig=",
            "signed",
            "yt-dlp",
            "stderr",
            "--cookies",
            "mpv-cookies",
            "cookies.txt",
            "yt-dlp.exe",
            "cli_path",
            "ytdl_cli",
        ] {
            assert!(!text.contains(banned), "banned {banned}: {text}");
        }
        // Page URL, chapters, cues, and timing survive sanitization.
        assert!(text.contains("youtube:e2e"), "media id: {text}");
        assert!(text.contains("watch?v=e2e"), "page url: {text}");
        assert!(text.contains("intro"), "chapter: {text}");
        assert!(text.contains("hello"), "cue: {text}");
        assert!(text.contains("online:en"), "choice: {text}");
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
                media_title: None,
                library_root: Some(r"D:\library".into()),
                group_key: Some("Demo.Show".into()),
                season: Some(1),
                episode: Some(1),
                position_ms: 12_000,
                duration_ms: None,
                sent_at_ms: 1,
                subtitle_choice_id: Some("embedded:2".into()),
                transcript_window_radius_sec: None,
            }),
            current_episode: Some(CurrentEpisodeLite {
                season: Some(1),
                episode: Some(1),
                title: Some("开场".into()),
                overview: Some("两人因纪录片重逢。".into()),
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
                subtitle_workshop_enabled: false,
                video_annotations_enabled: true,
            }),
            online: None,
            updated_at_ms: 1,
        };
        write_snapshot(&path, &snapshot).expect("write");
        let loaded = read_snapshot(&path).expect("read");
        assert_eq!(loaded, snapshot);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn chapter_task_snapshot_roundtrips_as_a_scoped_sidecar() {
        let dir =
            std::env::temp_dir().join(format!("lumina-chapter-snapshot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("chapter-agent-context.json");
        let context = ChapterTaskContext {
            task_id: 7,
            attempt_id: 3,
            episode_id: 11,
            database_path: r"C:\data\lumina.sqlite3".into(),
            media_path: r"C:\videos\episode.mkv".into(),
            duration_ms: 90_000,
            spoiler_boundary: "episode".into(),
            prompt_version: "chapter-v1".into(),
        };
        write_chapter_task_snapshot(&path, &LuminaMcpSnapshot::empty(), &context)
            .expect("write chapter snapshot");
        assert_eq!(
            read_chapter_task_context(&path).expect("read context"),
            Some(context)
        );
        assert_eq!(
            read_snapshot(&path)
                .expect("read regular snapshot")
                .schema_version,
            SNAPSHOT_SCHEMA_VERSION
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn syncing_chapter_snapshot_preserves_task_scope() {
        let dir = std::env::temp_dir().join(format!(
            "lumina-chapter-sync-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("chapter-agent-context.json");
        let context = ChapterTaskContext {
            task_id: 21,
            attempt_id: 22,
            episode_id: 23,
            database_path: r"C:\data\lumina.sqlite3".into(),
            media_path: r"C:\videos\episode.mkv".into(),
            duration_ms: 120_000,
            spoiler_boundary: "episode".into(),
            prompt_version: "chapter-v1".into(),
        };

        write_chapter_task_snapshot(&path, &LuminaMcpSnapshot::empty(), &context)
            .expect("write chapter snapshot");
        sync_snapshot_capabilities(&path, true).expect("sync chapter snapshot");

        assert_eq!(
            read_chapter_task_context(&path).expect("read chapter scope"),
            Some(context)
        );
        assert!(
            read_snapshot(&path)
                .expect("read synced snapshot")
                .capabilities
                .expect("capabilities")
                .vision_capable
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn syncing_ordinary_snapshot_drops_stale_chapter_scope() {
        let dir = std::env::temp_dir().join(format!(
            "lumina-chat-sync-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("agent-context.json");
        let context = ChapterTaskContext {
            task_id: 31,
            attempt_id: 32,
            episode_id: 33,
            database_path: r"C:\data\lumina.sqlite3".into(),
            media_path: r"C:\videos\episode.mkv".into(),
            duration_ms: 120_000,
            spoiler_boundary: "episode".into(),
            prompt_version: "chapter-v1".into(),
        };

        write_chapter_task_snapshot(&path, &LuminaMcpSnapshot::empty(), &context)
            .expect("write stale snapshot");
        sync_snapshot_capabilities(&path, false).expect("sync ordinary snapshot");

        assert_eq!(
            read_chapter_task_context(&path).expect("read ordinary scope"),
            None
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_anchor_without_transcript_preference_defaults_to_none() {
        let legacy = serde_json::json!({
            "mediaPath": "D:/videos/legacy.mkv",
            "mediaTitle": null,
            "libraryRoot": null,
            "groupKey": null,
            "season": null,
            "episode": null,
            "positionMs": 1000,
            "durationMs": null,
            "sentAtMs": 1,
            "subtitleChoiceId": null
        });
        let anchor: PromptAnchor = serde_json::from_value(legacy).expect("legacy anchor");
        assert_eq!(anchor.transcript_window_radius_sec, None);
    }
}
