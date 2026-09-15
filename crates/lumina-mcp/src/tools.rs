//! MCP tool handlers — read snapshot anchor and load heavy context on demand.

use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use tracing::warn;

use crate::snapshot::{ephemeral_tmp_dir, resolve_snapshot_path, LuminaMcpSnapshot, PromptAnchor};
use lumina_library::MergedMediaContext;
use lumina_library::{
    episode_index_for_group, load_context_at_root, load_context_for_group, load_library_index,
    resolve_episode_media_file, resolve_media_in_index, series_cache_from_context,
};
use lumina_media::frame_capture::{
    capture_frames, detect_scene_times, sample_times_for_window, select_keyframes,
    DEFAULT_FRAME_BUDGET, DEFAULT_SCENE_THRESHOLD,
};
use lumina_notes::proposal::{build_proposal, save_latest_proposal};
use lumina_subtitle::model::{Cue, Transcript};
use lumina_subtitle::write;
use lumina_subtitle::SubtitleService;

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
    use lumina_core::tool_contract as contract;
    let result = if name == contract::TOOL_PLAYBACK_CONTEXT {
        playback_context(snapshot)
    } else if name == contract::TOOL_LIBRARY_CONTEXT {
        library_context(snapshot)
    } else if name == contract::TOOL_EPISODE_INDEX {
        episode_index(snapshot)
    } else if name == contract::TOOL_TRANSCRIPT_WINDOW {
        transcript_window(snapshot, args)
    } else if name == contract::TOOL_EPISODE_TRANSCRIPT {
        episode_transcript(snapshot, args)
    } else if name == contract::TOOL_SUBTITLE_CUES || name == contract::TOOL_WRITE_SUBTITLE_TRACK {
        if !subtitle_workshop_enabled(snapshot) {
            Err("该工具仅对字幕制作助手开放".to_string())
        } else if name == contract::TOOL_SUBTITLE_CUES {
            subtitle_cues(snapshot, args)
        } else {
            write_subtitle_track(snapshot, args)
        }
    } else if name == contract::TOOL_CAPTURE_FRAMES {
        capture_frame_tool(snapshot, args)
    } else if name == contract::TOOL_AUDIO_MARKS {
        audio_marks(snapshot, args)
    } else if name == contract::TOOL_PROPOSE_ANNOTATION {
        if !video_annotations_enabled(snapshot) {
            Err("该工具未对当前会话开放".to_string())
        } else {
            propose_video_annotation(snapshot, args)
        }
    } else {
        Err(format!("Unknown tool: {name}"))
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

pub fn subtitle_workshop_enabled(snapshot: &LuminaMcpSnapshot) -> bool {
    snapshot
        .capabilities
        .as_ref()
        .map(|caps| caps.subtitle_workshop_enabled)
        .unwrap_or(false)
}

pub fn video_annotations_enabled(snapshot: &LuminaMcpSnapshot) -> bool {
    snapshot
        .capabilities
        .as_ref()
        .map(|caps| caps.video_annotations_enabled)
        .unwrap_or(true)
}

fn playback_context(snapshot: &LuminaMcpSnapshot) -> Result<Value, String> {
    let online = snapshot.online.as_ref().map(|online| {
        json!({
            "mediaId": online.media_id,
            "title": online.title,
            "durationMs": online.duration_ms,
            "webpageUrl": online.webpage_url,
            "extractor": online.extractor,
            "chapters": online.chapters,
            "subtitles": online.subtitles,
            "transcriptAvailable": online.transcript.is_some(),
        })
    });
    text_result(&json!({
        "anchor": snapshot.anchor,
        "currentEpisode": snapshot.current_episode,
        "session": snapshot.session,
        "libraryCached": snapshot.library.is_some(),
        "online": online,
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
    let duration_ms = snapshot_duration_ms(snapshot);
    let center_ms = parse_time_center_ms(args, anchor.position_ms, duration_ms);
    let choice_id = resolve_subtitle_choice_id(args, anchor)?;
    let lines = if let Some(online) = snapshot.online.as_ref() {
        let transcript = online
            .transcript
            .as_ref()
            .filter(|transcript| transcript.choice_id == choice_id)
            .ok_or_else(|| "当前在线字幕尚未缓存，请先在文稿面板选择字幕后重试".to_string())?;
        transcript_lines_from_cues(&transcript.cues, center_ms, before_sec, after_sec)
    } else {
        let media_path = PathBuf::from(&anchor.media_path);
        fetch_transcript_lines(&media_path, &choice_id, center_ms, before_sec, after_sec)?
    };
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
    let default_center = episode_transcript_default_center(anchor, season, episode);
    let center_ms = parse_time_center_ms(args, default_center, None);
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

fn subtitle_cues(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    let anchor = require_anchor(snapshot)?;
    let choice_id = resolve_subtitle_choice_id(args, anchor)?;
    // Canonical paging bounds live in `lumina-core`.
    let offset =
        lumina_core::tool_contract::clamp_cues_offset(args.get("offset").and_then(Value::as_u64));
    let limit =
        lumina_core::tool_contract::clamp_cues_limit(args.get("limit").and_then(Value::as_u64));
    // Online path reuses the same cached transcript as Transcript UI and
    // `lumina_get_transcript_window`: no file IO, no implicit resolve, no
    // signed URL / cookie / absolute path in the result.
    if let Some(online) = snapshot.online.as_ref() {
        if !online.subtitles.iter().any(|choice| choice.id == choice_id) {
            return Err("当前未选择可解析字幕".to_string());
        }
        let transcript = online
            .transcript
            .as_ref()
            .filter(|transcript| transcript.choice_id == choice_id)
            .ok_or_else(|| "当前在线字幕尚未缓存，请先在文稿面板选择字幕后重试".to_string())?;
        return paged_cues_result(&choice_id, &transcript.cues, offset, limit);
    }
    let media_path = PathBuf::from(&anchor.media_path);
    let transcript = load_tool_transcript(&media_path, &choice_id)?;
    paged_cues_result(&choice_id, &transcript.cues, offset, limit)
}

fn paged_cues_result(
    choice_id: &str,
    cues: &[Cue],
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    let total = cues.len();
    let (start, count, has_more) = lumina_core::tool_contract::paginate(total, offset, limit);
    let slice: Vec<_> = cues
        .iter()
        .skip(start)
        .take(count)
        .map(|cue| {
            json!({
                "index": cue.index,
                "startMs": cue.start_ms,
                "endMs": cue.end_ms,
                "text": cue.text,
            })
        })
        .collect();
    text_result(&json!({
        "choiceId": choice_id,
        "total": total,
        "offset": offset,
        "limit": limit,
        "cues": slice,
        "hasMore": has_more,
    }))
}

fn propose_video_annotation(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    let anchor = require_anchor(snapshot)?;
    let body = args
        .get("body")
        .and_then(Value::as_str)
        .ok_or_else(|| "缺少 body".to_string())?;
    let position_ms = args
        .get("positionMs")
        .and_then(Value::as_u64)
        .unwrap_or(anchor.position_ms);
    let include_quotes = args
        .get("includeQuotes")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let anchor_cue_index = args
        .get("anchorCueIndex")
        .and_then(Value::as_u64)
        .map(|value| value as u32);
    let quote_cue_indices = args.get("quoteCueIndices").and_then(|value| {
        value.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_u64().map(|index| index as u32))
                .collect::<Vec<_>>()
        })
    });
    let quote_hint = args
        .get("quoteHint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let subtitle_choice_id = args
        .get("subtitleChoiceId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| anchor.subtitle_choice_id.clone());

    let proposal = build_proposal(
        &anchor.media_path,
        position_ms,
        body,
        subtitle_choice_id,
        anchor_cue_index,
        quote_cue_indices.filter(|items| !items.is_empty()),
        quote_hint,
        include_quotes,
    )?;

    let workspace = workspace_from_snapshot_path()?;
    save_latest_proposal(&workspace, &proposal)?;

    text_result(&json!({
        "status": "pending_confirmation",
        "proposalId": proposal.proposal_id,
        "message": "批注提议已生成，请用户在 Lumina 界面确认后再写入笔记库。",
        "previewMarkdown": proposal.preview_markdown,
    }))
}

fn workspace_from_snapshot_path() -> Result<PathBuf, String> {
    let snapshot_path =
        resolve_snapshot_path().ok_or_else(|| "无法定位 Agent 工作目录".to_string())?;
    snapshot_path
        .parent()
        .and_then(|path| path.parent())
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位 Agent 工作目录".to_string())
}

fn write_subtitle_track(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    let anchor = require_anchor(snapshot)?;
    let lang = args
        .get("lang")
        .and_then(Value::as_str)
        .ok_or_else(|| "缺少 lang".to_string())?;
    let cues = parse_write_cues(args)?;
    let media_path = PathBuf::from(&anchor.media_path);
    let transcript = write::export_sidecar_srt(&media_path, lang, &cues)
        .map_err(|error| error.message.clone())?;
    text_result(&json!({
        "choiceId": transcript.choice_id,
        "language": transcript.language,
        "path": transcript.source_path,
        "cueCount": transcript.cues.len(),
    }))
}

fn parse_write_cues(args: &Value) -> Result<Vec<Cue>, String> {
    let raw = args
        .get("cues")
        .and_then(Value::as_array)
        .ok_or_else(|| "缺少 cues".to_string())?;
    // Canonical batch bounds live in `lumina-core`; wording stays here.
    match lumina_core::tool_contract::validate_write_cues_len(raw.len()) {
        Ok(()) => {}
        Err(lumina_core::tool_contract::WriteCuesError::Empty) => {
            return Err("cues 不能为空".into());
        }
        Err(lumina_core::tool_contract::WriteCuesError::TooMany) => {
            return Err("单次写入字幕过多，请分批".into());
        }
    }
    let mut cues = Vec::with_capacity(raw.len());
    for (i, item) in raw.iter().enumerate() {
        let start_ms = item
            .get("startMs")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("cues[{i}] 缺少 startMs"))?;
        let end_ms = item
            .get("endMs")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("cues[{i}] 缺少 endMs"))?;
        let text = item
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() {
            return Err(format!("cues[{i}] 文本为空"));
        }
        let index = item
            .get("index")
            .and_then(Value::as_u64)
            .map(|value| value as u32)
            .unwrap_or((i + 1) as u32);
        cues.push(Cue {
            index,
            start_ms,
            end_ms: end_ms.max(start_ms),
            text,
        });
    }
    Ok(cues)
}

/// Load a transcript for MCP tools, mirroring the UI command dispatcher
/// (`subtitle_load_choice`): downloaded `cache:<provider>:<lang>` choices live
/// in the process cache, not beside the media, so they must route to the
/// download cache instead of `SubtitleService` (which only knows
/// `embedded:` / `sidecar:` and would report "choice not found").
fn load_tool_transcript(media_path: &Path, choice_id: &str) -> Result<Transcript, String> {
    if lumina_ytdl::provider::parse_cache_choice(choice_id).is_some() {
        lumina_ytdl::provider::load_cached_choice(&media_path.to_string_lossy(), choice_id)
            .map_err(|error| error.message.clone())
    } else {
        SubtitleService::load_choice(media_path, choice_id).map_err(|error| error.message.clone())
    }
}

fn fetch_transcript_lines(
    media_path: &Path,
    choice_id: &str,
    center_ms: u64,
    before_sec: u32,
    after_sec: u32,
) -> Result<Vec<TranscriptLine>, String> {
    let transcript = load_tool_transcript(media_path, choice_id)?;
    let start_ms = center_ms.saturating_sub(u64::from(before_sec) * 1000);
    let end_ms = center_ms.saturating_add(u64::from(after_sec) * 1000);
    Ok(transcript
        .cues
        .into_iter()
        .filter(|cue| cue.end_ms > start_ms && cue.start_ms < end_ms)
        .map(|cue| TranscriptLine {
            start_ms: cue.start_ms,
            text: cue.text,
        })
        .collect())
}

fn transcript_lines_from_cues(
    cues: &[Cue],
    center_ms: u64,
    before_sec: u32,
    after_sec: u32,
) -> Vec<TranscriptLine> {
    let start_ms = center_ms.saturating_sub(u64::from(before_sec) * 1000);
    let end_ms = center_ms.saturating_add(u64::from(after_sec) * 1000);
    cues.iter()
        .filter(|cue| cue.end_ms > start_ms && cue.start_ms < end_ms)
        .map(|cue| TranscriptLine {
            start_ms: cue.start_ms,
            text: cue.text.clone(),
        })
        .collect()
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
    // Canonical bounds live in `lumina-core`; wording stays here.
    match lumina_core::tool_contract::validate_season_episode(
        args.get("season").and_then(Value::as_u64),
        args.get("episode").and_then(Value::as_u64),
    ) {
        Ok(pair) => Ok(pair),
        Err(lumina_core::tool_contract::SeasonEpisodeError::MissingSeason) => {
            Err("缺少 season".to_string())
        }
        Err(lumina_core::tool_contract::SeasonEpisodeError::MissingEpisode) => {
            Err("缺少 episode".to_string())
        }
        Err(lumina_core::tool_contract::SeasonEpisodeError::NonPositive) => {
            Err("season 与 episode 须大于 0".into())
        }
    }
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
    let duration_ms = snapshot_duration_ms(snapshot);
    let center_ms = parse_time_center_ms(args, anchor.position_ms, duration_ms);
    let coverage = sample_times_for_window(center_ms, duration_ms, before_sec, after_sec);
    // Scene mode merges detected cuts into the grid, still capped by budget.
    // Detection failure degrades to uniform coverage, never to an error.
    // `sample_times` are seconds; the tool labels below print them as-is.
    let sample_times = if parse_capture_mode(args) {
        let window_start = coverage.first().copied().unwrap_or(0.0);
        let window_end = coverage.last().copied().unwrap_or(f64::MAX);
        let scenes: Vec<f64> = detect_scene_times(&media_path, parse_scene_threshold(args))
            .unwrap_or_default()
            .into_iter()
            .filter(|time| *time >= window_start && *time <= window_end)
            .collect();
        select_keyframes(&coverage, &scenes, DEFAULT_FRAME_BUDGET.max_frames)
    } else {
        coverage
    };
    let cwd = snapshot_cwd()?;
    let output_dir = ephemeral_tmp_dir(&cwd).join(format!("capture-{}", anchor.sent_at_ms));
    let frames = capture_frames(&media_path, &sample_times, &output_dir)
        .map_err(|_| "无法获取当前画面".to_string())?;

    let mut content = Vec::new();
    content.push(json!({
        "type": "text",
        "text": format!(
            "anchorMs: {}, centerMs: {}",
            anchor.position_ms, center_ms
        ),
    }));
    for (index, frame) in frames.iter().enumerate() {
        let bytes = fs::read(frame).map_err(|_| "无法获取当前画面".to_string())?;
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

/// Non-semantic audio signals around a time point (P7-M2): silence intervals
/// and loudness-spike candidates. Never laughter/applause/music labels —
/// the detectors cannot distinguish them (review decision).
fn audio_marks(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    use lumina_media::audio_marks::{
        detect_loudness_peaks, detect_silence, DEFAULT_SILENCE_MIN_SEC, DEFAULT_SILENCE_NOISE_DB,
    };

    let anchor = require_anchor(snapshot)?;
    let media_path = PathBuf::from(&anchor.media_path);
    if !media_path.is_file() {
        return Err("无法读取当前媒体音频".to_string());
    }
    let duration_ms = snapshot_duration_ms(snapshot);
    let center_ms = parse_time_center_ms(args, anchor.position_ms, duration_ms);
    // Same window language as the transcript tools; audio-only decode is cheap
    // but still bounded (300 s per side, default 60/60).
    let (before_sec, after_sec) = parse_window_args(args, 60, 60);
    let from_sec = center_ms.saturating_sub(before_sec.saturating_mul(1000) as u64) as f64 / 1000.0;
    let to_sec = match duration_ms {
        Some(duration) => {
            ((center_ms + after_sec.saturating_mul(1000) as u64).min(duration)) as f64 / 1000.0
        }
        None => center_ms as f64 / 1000.0 + after_sec as f64,
    };
    if to_sec <= from_sec {
        return Err("音频分析时间窗无效".to_string());
    }
    let silences = detect_silence(
        &media_path,
        from_sec,
        to_sec,
        DEFAULT_SILENCE_NOISE_DB,
        DEFAULT_SILENCE_MIN_SEC,
    )
    .map_err(|_| "无法分析当前媒体音频".to_string())?;
    let peaks = detect_loudness_peaks(&media_path, from_sec, to_sec)
        .map_err(|_| "无法分析当前媒体音频".to_string())?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&json!({
                "anchorMs": anchor.position_ms,
                "centerMs": center_ms,
                "silences": silences,
                "peaks": peaks,
            })).unwrap_or_default(),
        }],
        "isError": false
    }))
}

fn load_media_context(
    root: &Path,
    anchor: &PromptAnchor,
    media_path: &Path,
) -> Result<Option<lumina_library::MediaMetadataContext>, String> {
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

fn snapshot_duration_ms(snapshot: &LuminaMcpSnapshot) -> Option<u64> {
    snapshot
        .anchor
        .as_ref()
        .and_then(|anchor| anchor.duration_ms)
        .or_else(|| {
            snapshot
                .online
                .as_ref()
                .and_then(|online| online.duration_ms)
        })
}

/// Shared time center for transcript + capture tools. Defaults to `default_center_ms`
/// (usually the frozen prompt anchor) unless `centerMs` / `atSec` is provided.
fn parse_time_center_ms(args: &Value, default_center_ms: u64, duration_ms: Option<u64>) -> u64 {
    lumina_core::tool_contract::resolve_center_ms(
        args.get("centerMs").and_then(Value::as_u64),
        args.get("atSec").and_then(Value::as_u64),
        default_center_ms,
        duration_ms,
    )
}

fn episode_transcript_default_center(anchor: &PromptAnchor, season: u32, episode: u32) -> u64 {
    if anchor.season == Some(season) && anchor.episode == Some(episode) {
        anchor.position_ms
    } else {
        0
    }
}

fn parse_window_args(args: &Value, default_before: u32, default_after: u32) -> (u32, u32) {
    // Canonical bounds live in `lumina-core`; this adapter only extracts
    // transport args so `tools/call` stays byte-identical.
    lumina_core::tool_contract::resolve_window(
        args.get("beforeSec").and_then(Value::as_u64),
        args.get("afterSec").and_then(Value::as_u64),
        args.get("radiusSec").and_then(Value::as_u64),
        default_before,
        default_after,
    )
}

fn parse_capture_window_args(args: &Value) -> (u32, u32) {
    lumina_core::tool_contract::resolve_capture_window(
        args.get("beforeSec").and_then(Value::as_u64),
        args.get("afterSec").and_then(Value::as_u64),
        args.get("radiusSec").and_then(Value::as_u64),
    )
}

/// P7-M1 opt-in sampling: `"scene"` merges scene cuts into the coverage grid.
/// Anything else (or absent) keeps the uniform behavior unchanged.
fn parse_capture_mode(args: &Value) -> bool {
    lumina_core::tool_contract::is_scene_mode(args.get("mode").and_then(Value::as_str))
}

fn parse_scene_threshold(args: &Value) -> f32 {
    lumina_core::tool_contract::clamp_scene_threshold(
        args.get("sceneThreshold").and_then(Value::as_f64),
        DEFAULT_SCENE_THRESHOLD,
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
    let root = lumina_library::discover_library_root_for_media(&media_path)
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
    crate::snapshot::resolve_snapshot_path()
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
    use crate::snapshot::{AgentCapabilities, CONTEXT_FILE_ENV};

    #[test]
    fn capture_tool_refuses_without_vision() {
        let snapshot = LuminaMcpSnapshot {
            capabilities: Some(AgentCapabilities {
                vision_capable: false,
                subtitle_workshop_enabled: false,
                video_annotations_enabled: true,
            }),
            ..LuminaMcpSnapshot::empty()
        };
        let err = capture_frame_tool(&snapshot, &json!({})).expect_err("vision gate");
        assert!(err.contains("识图"));
    }

    /// P7-S1 chain shape: text(anchor) + image/text(frame label) pairs.
    /// Needs ffmpeg; SKIP otherwise (same convention as the codec matrix).
    #[test]
    fn capture_tool_returns_labeled_image_blocks() {
        let ffmpeg = match lumina_media::tools::resolve_ffmpeg() {
            Ok(path) => path,
            Err(_) => {
                eprintln!("SKIP capture chain: ffmpeg not vendored on this machine");
                return;
            }
        };
        let dir = std::env::temp_dir().join(format!("lumina-capture-chain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("chain temp dir");
        let media = dir.join("chain-20s.mp4");
        let status = lumina_media::process::command(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=20:size=640x360:rate=30",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-an",
            ])
            .arg(&media)
            .output()
            .expect("spawn ffmpeg");
        assert!(status.status.success(), "chain fixture failed to encode");

        // snapshot_cwd() follows LUMINA_MCP_CONTEXT_FILE; restore afterwards.
        let previous = std::env::var(CONTEXT_FILE_ENV).ok();
        std::env::set_var(
            CONTEXT_FILE_ENV,
            dir.join(".lumina").join("agent-context.json"),
        );
        let snapshot = LuminaMcpSnapshot {
            anchor: Some(PromptAnchor {
                media_path: media.to_string_lossy().into_owned(),
                media_title: None,
                library_root: None,
                group_key: None,
                season: None,
                episode: None,
                position_ms: 10_000,
                duration_ms: None,
                sent_at_ms: 7,
                subtitle_choice_id: None,
            }),
            capabilities: Some(AgentCapabilities {
                vision_capable: true,
                subtitle_workshop_enabled: false,
                video_annotations_enabled: true,
            }),
            ..LuminaMcpSnapshot::empty()
        };
        let result = capture_frame_tool(&snapshot, &json!({ "beforeSec": 2, "afterSec": 2 }));
        match previous {
            Some(value) => std::env::set_var(CONTEXT_FILE_ENV, value),
            None => std::env::remove_var(CONTEXT_FILE_ENV),
        }
        let value = result.expect("capture chain");
        let content = value
            .get("content")
            .and_then(Value::as_array)
            .expect("content blocks");
        // text(anchor) + 5 × (image + label).
        assert_eq!(content.len(), 1 + 5 * 2, "blocks: {content:?}");
        assert!(content[0]
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("anchorMs") && text.contains("centerMs")));
        let mut labels = Vec::new();
        for pair in content[1..].chunks(2) {
            let (image, label) = (&pair[0], &pair[1]);
            assert_eq!(image.get("type").and_then(Value::as_str), Some("image"));
            assert_eq!(
                image.get("mimeType").and_then(Value::as_str),
                Some("image/jpeg")
            );
            let data = image.get("data").and_then(Value::as_str).unwrap_or("");
            assert!(data.starts_with("/9j/"), "expected JPEG bytes");
            assert_eq!(label.get("type").and_then(Value::as_str), Some("text"));
            labels.push(
                label
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            );
        }
        assert_eq!(
            labels,
            vec![
                "frame 0 @ 8.0s",
                "frame 1 @ 9.0s",
                "frame 2 @ 10.0s",
                "frame 3 @ 11.0s",
                "frame 4 @ 12.0s",
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_capture_mode_and_threshold() {
        assert!(parse_capture_mode(&json!({ "mode": "scene" })));
        assert!(parse_capture_mode(&json!({ "mode": "Scene" })));
        assert!(!parse_capture_mode(&json!({})));
        assert!(!parse_capture_mode(&json!({ "mode": "uniform" })));
        assert_eq!(parse_scene_threshold(&json!({})), DEFAULT_SCENE_THRESHOLD);
        assert_eq!(
            parse_scene_threshold(&json!({ "sceneThreshold": 0.7 })),
            0.7
        );
        assert_eq!(
            parse_scene_threshold(&json!({ "sceneThreshold": 5.0 })),
            0.9
        );
        assert_eq!(
            parse_scene_threshold(&json!({ "sceneThreshold": -1.0 })),
            0.1
        );
        assert_eq!(
            parse_scene_threshold(&json!({ "sceneThreshold": "high" })),
            DEFAULT_SCENE_THRESHOLD
        );
    }

    /// P7-M1 scene chain: cuts merge into the grid, budget still caps.
    /// Needs ffmpeg; SKIP otherwise.
    #[test]
    fn capture_tool_scene_mode_merges_cuts() {
        let ffmpeg = match lumina_media::tools::resolve_ffmpeg() {
            Ok(path) => path,
            Err(_) => {
                eprintln!("SKIP scene chain: ffmpeg not vendored on this machine");
                return;
            }
        };
        let dir = std::env::temp_dir().join(format!("lumina-scene-chain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("chain temp dir");
        let media = dir.join("cut.mp4");
        let status = lumina_media::process::command(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:duration=2:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:duration=2:size=320x240:rate=10",
                "-filter_complex",
                "[0:v][1:v]concat=n=2:v=1",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-an",
            ])
            .arg(&media)
            .output()
            .expect("spawn ffmpeg");
        assert!(status.status.success(), "cut fixture failed to encode");

        let previous = std::env::var(CONTEXT_FILE_ENV).ok();
        std::env::set_var(
            CONTEXT_FILE_ENV,
            dir.join(".lumina").join("agent-context.json"),
        );
        let snapshot = LuminaMcpSnapshot {
            anchor: Some(PromptAnchor {
                media_path: media.to_string_lossy().into_owned(),
                media_title: None,
                library_root: None,
                group_key: None,
                season: None,
                episode: None,
                position_ms: 2_000,
                duration_ms: None,
                sent_at_ms: 9,
                subtitle_choice_id: None,
            }),
            capabilities: Some(AgentCapabilities {
                vision_capable: true,
                subtitle_workshop_enabled: false,
                video_annotations_enabled: true,
            }),
            ..LuminaMcpSnapshot::empty()
        };
        let result = capture_frame_tool(
            &snapshot,
            &json!({ "beforeSec": 2, "afterSec": 2, "mode": "scene" }),
        );
        match previous {
            Some(value) => std::env::set_var(CONTEXT_FILE_ENV, value),
            None => std::env::remove_var(CONTEXT_FILE_ENV),
        }
        let value = result.expect("scene chain");
        let content = value
            .get("content")
            .and_then(Value::as_array)
            .expect("content blocks");
        // text(anchor) + N × (image + label), N within budget and ≥ coverage.
        assert!(!content.is_empty());
        let images = content
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("image"))
            .count();
        assert!((1..=DEFAULT_FRAME_BUDGET.max_frames).contains(&images));
        let labels: Vec<String> = content
            .iter()
            .filter_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| {
                        block
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    })
                    .filter(|text| text.starts_with("frame "))
            })
            .collect();
        assert_eq!(labels.len(), images);
        let mut sorted = labels.clone();
        sorted.sort();
        assert_eq!(labels, sorted, "frame labels stay time-ordered");
        let _ = std::fs::remove_dir_all(&dir);
    }

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
    fn parse_time_center_defaults_to_anchor() {
        assert_eq!(
            parse_time_center_ms(&json!({}), 125_000, Some(3_600_000)),
            125_000
        );
    }

    #[test]
    fn parse_time_center_accepts_center_ms() {
        assert_eq!(
            parse_time_center_ms(&json!({ "centerMs": 90_000 }), 125_000, None),
            90_000
        );
    }

    #[test]
    fn parse_time_center_accepts_at_sec() {
        assert_eq!(
            parse_time_center_ms(&json!({ "atSec": 120 }), 125_000, None),
            120_000
        );
    }

    #[test]
    fn parse_time_center_prefers_center_ms_over_at_sec() {
        assert_eq!(
            parse_time_center_ms(&json!({ "centerMs": 60_000, "atSec": 120 }), 125_000, None,),
            60_000
        );
    }

    #[test]
    fn parse_time_center_clamps_to_duration() {
        assert_eq!(
            parse_time_center_ms(&json!({ "centerMs": 9_000_000 }), 125_000, Some(3_600_000)),
            3_600_000
        );
    }

    #[test]
    fn episode_transcript_default_center_matches_anchor_episode() {
        let anchor = PromptAnchor {
            media_path: "Show/S01E02.mkv".into(),
            media_title: None,
            library_root: None,
            group_key: Some("Show".into()),
            season: Some(1),
            episode: Some(2),
            position_ms: 88_000,
            duration_ms: None,
            sent_at_ms: 1,
            subtitle_choice_id: None,
        };
        assert_eq!(episode_transcript_default_center(&anchor, 1, 2), 88_000);
        assert_eq!(episode_transcript_default_center(&anchor, 1, 3), 0);
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

    #[test]
    fn online_transcript_lines_are_windowed_without_reading_a_media_path() {
        let cues = vec![
            Cue {
                index: 1,
                start_ms: 1_000,
                end_ms: 2_000,
                text: "before".into(),
            },
            Cue {
                index: 2,
                start_ms: 9_000,
                end_ms: 11_000,
                text: "active".into(),
            },
            Cue {
                index: 3,
                start_ms: 30_000,
                end_ms: 31_000,
                text: "after".into(),
            },
        ];
        let lines = transcript_lines_from_cues(&cues, 10_000, 2, 2);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "active");
    }

    fn online_snapshot_with_transcript() -> LuminaMcpSnapshot {
        use crate::snapshot::OnlineMediaSnapshot;
        use lumina_subtitle::{SubtitleChoice, SubtitleSource, Transcript};
        LuminaMcpSnapshot {
            anchor: Some(PromptAnchor {
                media_path: "https://www.youtube.com/watch?v=abc".into(),
                media_title: None,
                library_root: None,
                group_key: None,
                season: None,
                episode: None,
                position_ms: 10_000,
                duration_ms: None,
                sent_at_ms: 1,
                subtitle_choice_id: Some("online:en".into()),
            }),
            capabilities: Some(AgentCapabilities {
                vision_capable: false,
                subtitle_workshop_enabled: true,
                video_annotations_enabled: true,
            }),
            online: Some(OnlineMediaSnapshot {
                media_id: "youtube:abc".into(),
                title: Some("Demo".into()),
                duration_ms: Some(60_000),
                webpage_url: Some("https://www.youtube.com/watch?v=abc".into()),
                extractor: Some("youtube".into()),
                chapters: vec![],
                subtitles: vec![SubtitleChoice {
                    id: "online:en".into(),
                    source: SubtitleSource::Sidecar,
                    label: "在线 · en".into(),
                    supported: true,
                    stream_index: None,
                    external_path: None,
                    codec_name: Some("vtt".into()),
                    language: Some("en".into()),
                }],
                transcript: Some(Transcript {
                    source_path: "https://www.youtube.com/watch?v=abc".into(),
                    choice_id: "online:en".into(),
                    stream_index: None,
                    language: Some("en".into()),
                    codec_name: Some("vtt".into()),
                    cues: vec![
                        Cue {
                            index: 1,
                            start_ms: 1_000,
                            end_ms: 2_000,
                            text: "before".into(),
                        },
                        Cue {
                            index: 2,
                            start_ms: 9_000,
                            end_ms: 11_000,
                            text: "active".into(),
                        },
                        Cue {
                            index: 3,
                            start_ms: 30_000,
                            end_ms: 31_000,
                            text: "after".into(),
                        },
                    ],
                }),
            }),
            ..LuminaMcpSnapshot::empty()
        }
    }

    fn tool_text_payload(value: &Value) -> Value {
        let text = value
            .get("content")
            .and_then(Value::as_array)
            .and_then(|blocks| blocks.first())
            .and_then(|block| block.get("text"))
            .and_then(Value::as_str)
            .expect("text block");
        serde_json::from_str(text).expect("tool payload JSON")
    }

    #[test]
    fn online_transcript_window_uses_cached_cues_with_time_range() {
        let snapshot = online_snapshot_with_transcript();
        let value = transcript_window(&snapshot, &json!({ "beforeSec": 2, "afterSec": 2 }))
            .expect("online window");
        let payload = tool_text_payload(&value);
        let lines = payload
            .get("lines")
            .and_then(Value::as_array)
            .expect("lines");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].get("text").and_then(Value::as_str), Some("active"));
        assert_eq!(lines[0].get("startMs").and_then(Value::as_u64), Some(9_000));
        let text = serde_json::to_string(&value)
            .expect("serialize")
            .to_lowercase();
        assert!(!text.contains("signed"), "no signed URL: {text}");
        assert!(!text.contains("cookie"), "no cookie: {text}");
    }

    #[test]
    fn online_transcript_window_rejects_choice_mismatch_without_network() {
        let snapshot = online_snapshot_with_transcript();
        let err = transcript_window(&snapshot, &json!({ "subtitleChoiceId": "online:xx" }))
            .expect_err("mismatched choice");
        assert!(
            err.contains("尚未缓存") || err.contains("未选择"),
            "stable error: {err}"
        );
    }

    #[test]
    fn downloaded_cache_transcript_window_reads_process_cache() {
        // Regression: the Transcript UI auto-selects a downloaded track
        // (`cache:<provider>:<lang>`, living in the process cache, not beside
        // the media), and the AI context carries that same id. The tool must
        // resolve it like `subtitle_load_choice` does — previously it only
        // knew `embedded:` / `sidecar:` and answered "无法提取字幕".
        // The download cache root follows APPDATA on Windows; redirect it so
        // the test never touches the user's real subtitle cache.
        let dir =
            std::env::temp_dir().join(format!("lumina-mcp-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let previous = std::env::var_os("APPDATA");
        std::env::set_var("APPDATA", &dir);
        let outcome = (|| {
            let candidate = lumina_ytdl::provider::SubtitleCandidate {
                provider: "subdl".into(),
                language: "en".into(),
                release_name: "demo".into(),
                size_bytes: 60,
                format: "srt".into(),
                season: None,
                episode: None,
                download_url: "https://example.invalid/sub.srt".into(),
                cached: true,
            };
            let media = "D:\\movie\\demo-cache-test.mp4";
            let stored = lumina_ytdl::provider::store_download(
                media,
                &candidate,
                b"1\n00:00:01,000 --> 00:00:02,000\nHello downloaded\n",
            )
            .map_err(|error| error.message.clone())?;
            assert_eq!(stored.choice_id, "cache:subdl:en");
            let snapshot = LuminaMcpSnapshot {
                anchor: Some(PromptAnchor {
                    media_path: media.into(),
                    media_title: None,
                    library_root: None,
                    group_key: None,
                    season: None,
                    episode: None,
                    position_ms: 1_500,
                    duration_ms: None,
                    sent_at_ms: 1,
                    subtitle_choice_id: Some(stored.choice_id.clone()),
                }),
                ..LuminaMcpSnapshot::empty()
            };
            transcript_window(&snapshot, &json!({}))
        })();
        match previous {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }
        let _ = std::fs::remove_dir_all(&dir);
        let payload = tool_text_payload(&outcome.expect("cached window"));
        let lines = payload
            .get("lines")
            .and_then(Value::as_array)
            .expect("lines");
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].get("text").and_then(Value::as_str),
            Some("Hello downloaded")
        );
    }

    #[test]
    fn online_transcript_window_requires_cache_without_crawling() {
        let mut snapshot = online_snapshot_with_transcript();
        if let Some(online) = snapshot.online.as_mut() {
            online.transcript = None;
        }
        let err = transcript_window(&snapshot, &json!({})).expect_err("missing transcript");
        assert!(
            err.contains("尚未缓存"),
            "cache miss is a business error: {err}"
        );
    }

    #[test]
    fn online_subtitle_cues_paginates_with_same_semantics() {
        let snapshot = online_snapshot_with_transcript();
        let value = subtitle_cues(&snapshot, &json!({ "offset": 0, "limit": 2 }))
            .expect("online cues page 1");
        let payload = tool_text_payload(&value);
        assert_eq!(
            payload.get("choiceId").and_then(Value::as_str),
            Some("online:en")
        );
        assert_eq!(payload.get("total").and_then(Value::as_u64), Some(3));
        assert_eq!(
            payload.get("cues").and_then(Value::as_array).map(Vec::len),
            Some(2)
        );
        assert_eq!(payload.get("hasMore").and_then(Value::as_bool), Some(true));
        let first = payload
            .get("cues")
            .and_then(Value::as_array)
            .and_then(|cues| cues.first())
            .expect("first cue");
        assert_eq!(first.get("startMs").and_then(Value::as_u64), Some(1_000));
        assert_eq!(first.get("endMs").and_then(Value::as_u64), Some(2_000));
        assert_eq!(first.get("text").and_then(Value::as_str), Some("before"));

        let second = subtitle_cues(&snapshot, &json!({ "offset": 2, "limit": 2 }))
            .expect("online cues page 2");
        let payload = tool_text_payload(&second);
        assert_eq!(
            payload.get("cues").and_then(Value::as_array).map(Vec::len),
            Some(1)
        );
        assert_eq!(payload.get("hasMore").and_then(Value::as_bool), Some(false));
        let text = serde_json::to_string(&second)
            .expect("serialize")
            .to_lowercase();
        assert!(!text.contains("signed"), "no signed URL: {text}");
        assert!(!text.contains("cookie"), "no cookie: {text}");
    }

    #[test]
    fn online_subtitle_cues_validates_choice_and_cache() {
        let snapshot = online_snapshot_with_transcript();
        let err = subtitle_cues(
            &snapshot,
            &json!({ "subtitleChoiceId": "sidecar:/tmp/x.srt" }),
        )
        .expect_err("unknown choice");
        assert!(
            err.contains("未选择") || err.contains("尚未缓存"),
            "choice error: {err}"
        );

        let mut missing = online_snapshot_with_transcript();
        if let Some(online) = missing.online.as_mut() {
            online.transcript = None;
        }
        let err = subtitle_cues(&missing, &json!({})).expect_err("missing transcript");
        assert!(err.contains("尚未缓存"), "cache miss: {err}");
    }

    #[test]
    fn online_tool_results_hide_paths_urls_and_cookies() {
        let snapshot = online_snapshot_with_transcript();
        let window = transcript_window(&snapshot, &json!({})).expect("window");
        let cues = subtitle_cues(&snapshot, &json!({})).expect("cues");
        for value in [&window, &cues] {
            let text = serde_json::to_string(value)
                .expect("serialize")
                .to_lowercase();
            assert!(!text.contains("cookie"), "no cookie: {text}");
            assert!(!text.contains("sig="), "no signature: {text}");
            assert!(!text.contains("signed"), "no signed URL: {text}");
        }
        // Snapshot itself must not carry absolute cache paths for online tracks.
        let snap_json = serde_json::to_value(&snapshot.online).expect("online snapshot serializes");
        let snap_text = snap_json.to_string().to_lowercase();
        assert!(!snap_text.contains("cookie"), "snapshot: {snap_text}");
    }

    #[test]
    fn playback_context_preserves_page_url_without_sensitive_fields() {
        let snapshot = online_snapshot_with_transcript();
        let value = playback_context(&snapshot).expect("playback context");
        let payload = tool_text_payload(&value);
        let online = payload.get("online").expect("online block");
        assert_eq!(
            online.get("mediaId").and_then(Value::as_str),
            Some("youtube:abc")
        );
        assert_eq!(
            online.get("webpageUrl").and_then(Value::as_str),
            Some("https://www.youtube.com/watch?v=abc")
        );
        assert_eq!(
            online.get("transcriptAvailable").and_then(Value::as_bool),
            Some(true)
        );
        let text = serde_json::to_string(&value)
            .expect("serialize")
            .to_lowercase();
        for banned in [
            "cookie",
            "sig=",
            "signed",
            "stderr",
            "--cookies",
            "ytdl_cli",
            "cookies-file",
            "yt-dlp.exe",
        ] {
            assert!(!text.contains(banned), "banned {banned}: {text}");
        }
    }

    #[test]
    fn online_capture_returns_stable_error_without_file_access() {
        // Online anchor is a page URL, never a local file: capture must fail
        // with the stable business error, not a raw IO path.
        let mut snapshot = online_snapshot_with_transcript();
        snapshot.capabilities = Some(AgentCapabilities {
            vision_capable: true,
            subtitle_workshop_enabled: false,
            video_annotations_enabled: true,
        });
        let err = capture_frame_tool(&snapshot, &json!({})).expect_err("online capture");
        assert_eq!(err, "无法获取当前画面");
    }

    #[test]
    fn vision_gate_stays_closed_for_online_snapshot() {
        let mut snapshot = online_snapshot_with_transcript();
        snapshot.capabilities = Some(AgentCapabilities {
            vision_capable: false,
            subtitle_workshop_enabled: false,
            video_annotations_enabled: true,
        });
        assert!(!vision_capable(&snapshot));
        let err = capture_frame_tool(&snapshot, &json!({})).expect_err("vision gate");
        assert!(err.contains("识图"));
    }

    /// P7-M2 tool shape on a synthetic gap fixture. Needs ffmpeg; SKIP otherwise.
    #[test]
    fn audio_marks_tool_returns_silence_window() {
        let ffmpeg = match lumina_media::tools::resolve_ffmpeg() {
            Ok(path) => path,
            Err(_) => {
                eprintln!("SKIP audio marks tool: ffmpeg not vendored on this machine");
                return;
            }
        };
        let dir = std::env::temp_dir().join(format!("lumina-audio-tool-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tool temp dir");
        let media = dir.join("gap.m4a");
        let status = lumina_media::process::command(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=44100:cl=stereo:d=2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-filter_complex",
                "[0:a][1:a][2:a]concat=n=3:v=0:a=1",
                "-c:a",
                "aac",
                "-vn",
            ])
            .arg(&media)
            .output()
            .expect("spawn ffmpeg");
        assert!(status.status.success(), "gap fixture failed to encode");

        let snapshot = LuminaMcpSnapshot {
            anchor: Some(PromptAnchor {
                media_path: media.to_string_lossy().into_owned(),
                media_title: None,
                library_root: None,
                group_key: None,
                season: None,
                episode: None,
                position_ms: 3_000,
                duration_ms: None,
                sent_at_ms: 11,
                subtitle_choice_id: None,
            }),
            capabilities: Some(AgentCapabilities {
                vision_capable: false,
                subtitle_workshop_enabled: false,
                video_annotations_enabled: true,
            }),
            ..LuminaMcpSnapshot::empty()
        };
        let value =
            audio_marks(&snapshot, &json!({ "beforeSec": 3, "afterSec": 3 })).expect("audio marks");
        let text = value
            .get("content")
            .and_then(Value::as_array)
            .and_then(|blocks| blocks.first())
            .and_then(|block| block.get("text"))
            .and_then(Value::as_str)
            .expect("text block");
        let parsed: Value = serde_json::from_str(text).expect("marks JSON");
        let silences = parsed
            .get("silences")
            .and_then(Value::as_array)
            .expect("silences");
        assert_eq!(silences.len(), 1, "marks: {text}");
        let start = silences[0]
            .get("startMs")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let end = silences[0]
            .get("endMs")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        assert!((start as i64 - 2000).abs() < 400, "marks: {text}");
        assert!((end as i64 - 4000).abs() < 400, "marks: {text}");
        assert!(parsed.get("peaks").and_then(Value::as_array).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
