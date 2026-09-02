//! MCP tool handlers — read snapshot anchor and load heavy context on demand.

use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use tracing::warn;

use crate::library::MergedMediaContext;
use crate::library::{
    episode_index_for_group, load_context_at_root, load_context_for_group, load_library_index,
    resolve_episode_media_file, resolve_media_in_index, series_cache_from_context,
};
use crate::mcp::snapshot::{ephemeral_tmp_dir, LuminaMcpSnapshot, PromptAnchor};
use crate::media::frame_capture::{capture_frames, sample_times_for_window, MAX_CAPTURE_SPAN_SEC};
use crate::subtitle::SubtitleService;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptWindowResult {
    center_ms: u64,
    anchor_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    season: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_file_name: Option<String>,
    before_sec: u32,
    after_sec: u32,
    lines: Vec<TranscriptLine>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptLine {
    start_ms: u64,
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryContextResult {
    series_from_cache: bool,
    series: Value,
    episode: Option<Value>,
    merged: Option<MergedMediaContext>,
}

pub fn handle_tool_call(
    snapshot: &LuminaMcpSnapshot,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    let result = match name {
        "lumina_get_playback_context" => playback_context(snapshot),
        "lumina_get_library_context" => library_context(snapshot),
        "lumina_get_episode_index" => episode_index(snapshot),
        "lumina_get_transcript_window" => transcript_window(snapshot, args),
        "lumina_get_episode_transcript" => episode_transcript(snapshot, args),
        "lumina_capture_frames" => capture_frame_tool(snapshot, args),
        other => Err(format!("Unknown tool: {other}")),
    };
    if let Err(message) = result.as_ref() {
        warn!(tool = name, reason = %message, "lumina MCP tool failed");
    }
    result
}

pub fn vision_capable(snapshot: &LuminaMcpSnapshot) -> bool {
    snapshot
        .capabilities
        .as_ref()
        .map(|caps| caps.vision_capable)
        .unwrap_or(false)
}

fn playback_context(snapshot: &LuminaMcpSnapshot) -> Result<Value, String> {
    text_result(&json!({
        "anchor": snapshot.anchor,
        "playback": snapshot.playback,
        "session": snapshot.session,
        "libraryCached": snapshot.library.is_some(),
    }))
}

fn library_context(snapshot: &LuminaMcpSnapshot) -> Result<Value, String> {
    let anchor = require_anchor(snapshot)?;
    let (root, media_path) = resolve_paths(anchor)?;
    let series_from_cache = snapshot.library.is_some();
    let series = if let Some(cache) = snapshot.library.as_ref() {
        serde_json::to_value(cache).map_err(|error| error.to_string())?
    } else {
        let context = load_media_context(&root, anchor, &media_path)?
            .ok_or_else(|| "当前媒体暂无已匹配的元数据".to_string())?;
        serde_json::to_value(series_cache_from_context(&context))
            .map_err(|error| error.to_string())?
    };

    let full = load_media_context(&root, anchor, &media_path)?
        .ok_or_else(|| "当前媒体暂无已匹配的元数据".to_string())?;
    let episode = full.item.as_ref().map(|item| {
        json!({
            "title": item.title,
            "overview": item.overview,
            "season": item.season,
            "episode": item.episode,
        })
    });
    let payload = LibraryContextResult {
        series_from_cache,
        series,
        episode,
        merged: full.merged,
    };
    text_result(&payload)
}

fn episode_index(snapshot: &LuminaMcpSnapshot) -> Result<Value, String> {
    let anchor = require_anchor(snapshot)?;
    let (root, media_path) = resolve_paths(anchor)?;
    let group_key = resolve_group_key(&root, anchor, &media_path)?;
    let entries =
        episode_index_for_group(&root, &group_key).map_err(|error| error.message.clone())?;
    if entries.is_empty() {
        return Err("当前剧集暂无分集元数据".to_string());
    }
    text_result(&json!({ "episodes": entries }))
}

fn transcript_window(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    let anchor = require_anchor(snapshot)?;
    let (before_sec, after_sec) = parse_window_args(args, 60, 60);
    let duration_ms = snapshot
        .playback
        .as_ref()
        .and_then(|playback| playback.duration_ms);
    let center_ms = parse_transcript_center_ms(args, anchor.position_ms, duration_ms);
    let choice_id = resolve_subtitle_choice_id(args, anchor)?;
    let media_path = PathBuf::from(&anchor.media_path);
    let lines = fetch_transcript_lines(&media_path, &choice_id, center_ms, before_sec, after_sec)?;
    let payload = TranscriptWindowResult {
        center_ms,
        anchor_ms: anchor.position_ms,
        season: None,
        episode: None,
        media_file_name: None,
        before_sec,
        after_sec,
        lines,
    };
    text_result(&payload)
}

fn episode_transcript(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    let anchor = require_anchor(snapshot)?;
    let (season, episode) = parse_required_season_episode(args)?;
    let (before_sec, after_sec) = parse_window_args(args, 60, 60);
    let center_ms = parse_transcript_center_ms(args, 0, None);
    let choice_id = resolve_subtitle_choice_id(args, anchor)?;
    let (root, media_path) = resolve_paths(anchor)?;
    let group_key = resolve_group_key(&root, anchor, &media_path)?;
    let index = load_library_index(&root)
        .map_err(|error| error.message.clone())?
        .ok_or_else(|| "当前媒体未加入媒体库".to_string())?;
    let file = resolve_episode_media_file(&index, &group_key, season, episode)
        .map_err(|error| error.message.clone())?;
    let episode_media_path = root.join(&file.relative_path);
    if !episode_media_path.is_file() {
        return Err("无法打开该集媒体文件".to_string());
    }
    let lines = fetch_transcript_lines(
        &episode_media_path,
        &choice_id,
        center_ms,
        before_sec,
        after_sec,
    )?;
    let payload = TranscriptWindowResult {
        center_ms,
        anchor_ms: anchor.position_ms,
        season: Some(season),
        episode: Some(episode),
        media_file_name: Some(file.file_name.clone()),
        before_sec,
        after_sec,
        lines,
    };
    text_result(&payload)
}

fn fetch_transcript_lines(
    media_path: &Path,
    choice_id: &str,
    center_ms: u64,
    before_sec: u32,
    after_sec: u32,
) -> Result<Vec<TranscriptLine>, String> {
    let cues = SubtitleService::excerpt_in_range(
        media_path,
        choice_id,
        center_ms,
        u64::from(before_sec) * 1000,
        u64::from(after_sec) * 1000,
    )
    .map_err(|error| error.message.clone())?;
    Ok(cues
        .into_iter()
        .map(|cue| TranscriptLine {
            start_ms: cue.start_ms,
            text: cue.text,
        })
        .collect())
}

fn resolve_subtitle_choice_id(args: &Value, anchor: &PromptAnchor) -> Result<String, String> {
    if let Some(choice_id) = args
        .get("subtitleChoiceId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(choice_id.to_string());
    }
    anchor
        .subtitle_choice_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "当前未选择可解析字幕".to_string())
}

fn parse_required_season_episode(args: &Value) -> Result<(u32, u32), String> {
    let season = args
        .get("season")
        .and_then(Value::as_u64)
        .ok_or_else(|| "缺少 season".to_string())? as u32;
    let episode = args
        .get("episode")
        .and_then(Value::as_u64)
        .ok_or_else(|| "缺少 episode".to_string())? as u32;
    if season == 0 || episode == 0 {
        return Err("season 与 episode 须大于 0".into());
    }
    Ok((season, episode))
}

fn capture_frame_tool(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    if !vision_capable(snapshot) {
        return Err("当前模型不支持识图截图".to_string());
    }
    let anchor = require_anchor(snapshot)?;
    let (before_sec, after_sec) = parse_capture_window_args(args);
    let media_path = PathBuf::from(&anchor.media_path);
    if !media_path.is_file() {
        return Err("无法获取当前画面".to_string());
    }
    let duration_ms = snapshot
        .playback
        .as_ref()
        .and_then(|playback| playback.duration_ms);
    let sample_times =
        sample_times_for_window(anchor.position_ms, duration_ms, before_sec, after_sec);
    let cwd = snapshot_cwd()?;
    let output_dir = ephemeral_tmp_dir(&cwd).join(format!("capture-{}", anchor.sent_at_ms));
    let frames = capture_frames(&media_path, &sample_times, &output_dir)
        .map_err(|_| "无法获取当前画面".to_string())?;

    let mut content = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let bytes = fs::read(frame).map_err(|error| error.to_string())?;
        content.push(json!({
            "type": "image",
            "data": STANDARD.encode(bytes),
            "mimeType": "image/jpeg",
        }));
        content.push(json!({
            "type": "text",
            "text": format!("frame {index} @ {:.1}s", sample_times.get(index).copied().unwrap_or(0.0)),
        }));
    }
    let _ = fs::remove_dir_all(&output_dir);
    Ok(json!({
        "content": content,
        "isError": false
    }))
}

fn load_media_context(
    root: &Path,
    anchor: &PromptAnchor,
    media_path: &Path,
) -> Result<Option<crate::library::MediaMetadataContext>, String> {
    if let Some(context) =
        load_context_at_root(root, media_path).map_err(|error| error.message.clone())?
    {
        return Ok(Some(context));
    }
    let group_key = anchor
        .group_key
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let Some(group_key) = group_key else {
        return Ok(None);
    };
    load_context_for_group(root, group_key, media_path, anchor.season, anchor.episode)
        .map_err(|error| error.message.clone())
}

fn resolve_group_key(
    root: &Path,
    anchor: &PromptAnchor,
    media_path: &Path,
) -> Result<String, String> {
    if let Ok(Some(index)) = load_library_index(root) {
        if let Ok(Some((file, _group))) = resolve_media_in_index(&index, media_path, root) {
            return Ok(file.group_key.clone());
        }
    }
    if let Some(group_key) = anchor
        .group_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(group_key.to_string());
    }
    Err("当前媒体未加入媒体库".to_string())
}

fn parse_transcript_center_ms(
    args: &Value,
    anchor_ms: u64,
    duration_ms: Option<u64>,
) -> u64 {
    let center = if let Some(ms) = args.get("centerMs").and_then(Value::as_u64) {
        ms
    } else if let Some(sec) = args.get("atSec").and_then(Value::as_u64) {
        sec.saturating_mul(1000)
    } else {
        anchor_ms
    };
    duration_ms.map_or(center, |duration| center.min(duration))
}

fn parse_window_args(args: &Value, default_before: u32, default_after: u32) -> (u32, u32) {
    if let Some(radius) = args.get("radiusSec").and_then(Value::as_u64) {
        let radius = radius.min(300) as u32;
        return (radius, radius);
    }
    let before = args
        .get("beforeSec")
        .and_then(Value::as_u64)
        .map(|value| value.min(300) as u32)
        .unwrap_or(default_before);
    let after = args
        .get("afterSec")
        .and_then(Value::as_u64)
        .map(|value| value.min(300) as u32)
        .unwrap_or(default_after);
    (before, after)
}

fn parse_capture_window_args(args: &Value) -> (u32, u32) {
    let (before, after) = parse_window_args(args, 0, 0);
    (
        before.min(MAX_CAPTURE_SPAN_SEC),
        after.min(MAX_CAPTURE_SPAN_SEC),
    )
}

fn require_anchor(snapshot: &LuminaMcpSnapshot) -> Result<&PromptAnchor, String> {
    snapshot
        .anchor
        .as_ref()
        .filter(|anchor| !anchor.media_path.trim().is_empty())
        .ok_or_else(|| "当前没有可用的播放锚点".to_string())
}

fn resolve_paths(anchor: &PromptAnchor) -> Result<(PathBuf, PathBuf), String> {
    let media_path = PathBuf::from(&anchor.media_path);
    let root = crate::library::discover_library_root_for_media(&media_path)
        .or_else(|| {
            anchor
                .library_root
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
        })
        .ok_or_else(|| "当前媒体未关联媒体库目录".to_string())?;
    Ok((root, media_path))
}

fn snapshot_cwd() -> Result<PathBuf, String> {
    crate::mcp::snapshot::resolve_snapshot_path()
        .and_then(|path| {
            path.parent()
                .and_then(|parent| parent.parent())
                .map(Path::to_path_buf)
        })
        .ok_or_else(|| "无法定位会话目录".to_string())
}

fn text_result<T: Serialize>(payload: &T) -> Result<Value, String> {
    let text = serde_json::to_string_pretty(payload).map_err(|error| error.to_string())?;
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_radius_argument() {
        let (before, after) = parse_window_args(&json!({ "radiusSec": 3 }), 60, 60);
        assert_eq!((before, after), (3, 3));
    }

    #[test]
    fn parse_asymmetric_window() {
        let (before, after) = parse_window_args(&json!({ "beforeSec": 3, "afterSec": 2 }), 60, 60);
        assert_eq!((before, after), (3, 2));
    }

    #[test]
    fn parse_transcript_center_defaults_to_anchor() {
        assert_eq!(
            parse_transcript_center_ms(&json!({}), 125_000, Some(3_600_000)),
            125_000
        );
    }

    #[test]
    fn parse_transcript_center_accepts_center_ms() {
        assert_eq!(
            parse_transcript_center_ms(&json!({ "centerMs": 90_000 }), 125_000, None),
            90_000
        );
    }

    #[test]
    fn parse_transcript_center_accepts_at_sec() {
        assert_eq!(
            parse_transcript_center_ms(&json!({ "atSec": 120 }), 125_000, None),
            120_000
        );
    }

    #[test]
    fn parse_transcript_center_prefers_center_ms_over_at_sec() {
        assert_eq!(
            parse_transcript_center_ms(
                &json!({ "centerMs": 60_000, "atSec": 120 }),
                125_000,
                None,
            ),
            60_000
        );
    }

    #[test]
    fn parse_transcript_center_clamps_to_duration() {
        assert_eq!(
            parse_transcript_center_ms(&json!({ "centerMs": 9_000_000 }), 125_000, Some(3_600_000)),
            3_600_000
        );
    }

    #[test]
    fn parse_required_season_episode_rejects_missing_fields() {
        assert!(parse_required_season_episode(&json!({})).is_err());
        assert!(parse_required_season_episode(&json!({ "season": 1 })).is_err());
    }

    #[test]
    fn parse_required_season_episode_accepts_positive_values() {
        assert_eq!(
            parse_required_season_episode(&json!({ "season": 2, "episode": 5 })).expect("values"),
            (2, 5)
        );
    }
}
