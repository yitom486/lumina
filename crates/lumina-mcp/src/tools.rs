//! MCP tool handlers — read snapshot anchor and load heavy context on demand.

use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "test-support")]
const TEST_CAPTURE_FIXTURE_DIR_ENV: &str = "LUMINA_MCP_TEST_CAPTURE_FIXTURE_DIR";

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use tracing::warn;

use crate::snapshot::{
    ephemeral_tmp_dir, resolve_snapshot_path, ChapterTaskContext, LuminaMcpSnapshot, PromptAnchor,
};
use lumina_library::MergedMediaContext;
use lumina_library::{
    episode_index_for_group, load_context_at_root, load_context_for_group, load_library_index,
    resolve_episode_media_file, resolve_media_in_index, series_cache_from_context, Database,
    NewChapter, NewChapterAsset, NewChapterRevision, NewQuestionCandidate, NewWatchFeedItem,
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

pub fn handle_tool_call_with_context(
    snapshot: &LuminaMcpSnapshot,
    chapter_task: Option<&ChapterTaskContext>,
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
    } else if name == contract::TOOL_ENTITY_TIMELINE {
        entity_timeline(snapshot, args)
    } else if name == contract::TOOL_SEEK_PLAYBACK {
        seek_playback(snapshot, args)
    } else if name == contract::TOOL_PROPOSE_ANNOTATION {
        if !video_annotations_enabled(snapshot) {
            Err("该工具未对当前会话开放".to_string())
        } else {
            propose_video_annotation(snapshot, args)
        }
    } else if name == contract::TOOL_CREATE_CHAPTER_OUTLINE {
        chapter_task
            .ok_or_else(|| "章节任务上下文不可用".to_string())
            .and_then(|context| create_chapter_outline(context, args))
    } else if name == contract::TOOL_CAPTURE_CHAPTER_EVIDENCE {
        chapter_task
            .ok_or_else(|| "章节任务上下文不可用".to_string())
            .and_then(|context| capture_chapter_evidence(context, args))
    } else if name == contract::TOOL_UPDATE_CHAPTER_DRAFT {
        chapter_task
            .ok_or_else(|| "章节任务上下文不可用".to_string())
            .and_then(|context| update_chapter_draft(context, args))
    } else if name == contract::TOOL_FINALIZE_CHAPTER_TASK {
        chapter_task
            .ok_or_else(|| "章节任务上下文不可用".to_string())
            .and_then(|context| finalize_chapter_task(context, args))
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
    let preferred_radius_sec = anchor
        .transcript_window_radius_sec
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(60);
    let (before_sec, after_sec) =
        parse_window_args(args, preferred_radius_sec, preferred_radius_sec);
    tracing::debug!(
        anchor_ms = anchor.position_ms,
        preference_radius_sec = ?anchor.transcript_window_radius_sec,
        before_sec,
        after_sec,
        "resolved transcript window"
    );
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

/// 单集扫描输入：来自索引的集信息 + 该集解析出的台词（或失败原因）。
struct EpisodeScanInput {
    season: u32,
    episode: u32,
    #[allow(dead_code)]
    media_file_name: String,
    transcript: Result<Transcript, String>,
    /// 用户已确认的批注/笔记（body 截断防膨胀）：语义知识只来自
    /// 用户确认过的内容，AI 永远没有直写路径。
    notes: Vec<(u64, String)>,
}

struct EntityHit {
    season: u32,
    episode: u32,
    first_seen_ms: u64,
    occurrences: u32,
}

/// 单次时间线扫描最多解析的集数（防长剧全季拉爆 IO）。
const ENTITY_TIMELINE_MAX_EPISODES: usize = 60;

/// 用户确认的批注（语义知识，与机械聚合的 derived 数据分开承载）。
struct EntityAnnotation {
    season: u32,
    episode: u32,
    position_ms: u64,
    body: String,
}

struct EntityAggregated {
    name: String,
    hits: Vec<EntityHit>,
    annotations: Vec<EntityAnnotation>,
}

/// 纯函数聚合：名字 × 台词集上做大小写不敏感子串匹配，输出逐集命中
/// （首次出现锚点 + 次数）、无法加载字幕的集，以及正文提到该名字的
/// 用户确认批注。台词命中是 derived（机械）；批注是用户确认的知识，
/// 只读回带，AI 没有任何直写路径。名字由调用方（模型）从上下文挑选。
fn aggregate_entity_timeline(
    names: &[String],
    episodes: &[EpisodeScanInput],
) -> (Vec<EntityAggregated>, Vec<(u32, u32, String)>) {
    let mut entities: Vec<EntityAggregated> = names
        .iter()
        .map(|name| EntityAggregated {
            name: name.trim().to_string(),
            hits: Vec::new(),
            annotations: Vec::new(),
        })
        .collect();
    let mut skipped: Vec<(u32, u32, String)> = Vec::new();

    for entry in episodes {
        if let Err(reason) = &entry.transcript {
            skipped.push((entry.season, entry.episode, reason.clone()));
        }
        for (index, name) in names.iter().enumerate() {
            let needle = name.trim().to_lowercase();
            if needle.is_empty() {
                continue;
            }
            // 用户确认批注：正文提到该名字即回带（独立于台词命中）。
            for (position_ms, body) in &entry.notes {
                if body.to_lowercase().contains(&needle)
                    && entities[index].annotations.len() < ENTITY_TIMELINE_MAX_NOTES
                {
                    entities[index].annotations.push(EntityAnnotation {
                        season: entry.season,
                        episode: entry.episode,
                        position_ms: *position_ms,
                        body: body.clone(),
                    });
                }
            }
            if let Ok(transcript) = &entry.transcript {
                let mut occurrences = 0u32;
                let mut first_seen_ms = None;
                for cue in &transcript.cues {
                    if cue.text.to_lowercase().contains(&needle) {
                        occurrences += 1;
                        if first_seen_ms.is_none() {
                            first_seen_ms = Some(cue.start_ms);
                        }
                    }
                }
                if occurrences > 0 {
                    entities[index].hits.push(EntityHit {
                        season: entry.season,
                        episode: entry.episode,
                        first_seen_ms: first_seen_ms.unwrap_or(0),
                        occurrences,
                    });
                }
            }
        }
    }
    (entities, skipped)
}

/// 全季「角色出场时间线」：逐集检索台词里的名字出现情况。纯只读聚合
/// （derived=true 标明非语义），数据不足时明说，禁止编造。台词解析是
/// IO 重操作 → heavy limiter。仅本地媒体库会话可用（在线会话无文件库）。
fn entity_timeline(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    let anchor = require_anchor(snapshot)?;
    let names: Vec<String> = args
        .get("names")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return Err("请提供要检索的人物名（names）".to_string());
    }
    let names: Vec<String> = names.into_iter().take(8).collect();
    let choice_id = resolve_subtitle_choice_id(args, anchor)?;
    let (root, media_path) = resolve_paths(anchor)?;
    let group_key = resolve_group_key(&root, anchor, &media_path)?;
    let index = load_library_index(&root)
        .map_err(|error| error.message.clone())?
        .ok_or_else(|| "当前媒体未加入媒体库".to_string())?;

    let mut files: Vec<_> = index
        .files
        .iter()
        .filter(|file| file.group_key == group_key && file.episode.is_some())
        .collect();
    files.sort_by_key(|file| {
        (
            file.season.unwrap_or(1),
            file.episode.unwrap_or(0),
            file.file_name.clone(),
        )
    });
    if files.len() > ENTITY_TIMELINE_MAX_EPISODES {
        files.truncate(ENTITY_TIMELINE_MAX_EPISODES);
    }

    let scans: Vec<EpisodeScanInput> = files
        .iter()
        .map(|file| {
            let episode_media_path = root.join(&file.relative_path);
            EpisodeScanInput {
                season: file.season.unwrap_or(1),
                episode: file.episode.unwrap_or(0),
                media_file_name: file.file_name.clone(),
                transcript: load_tool_transcript(&episode_media_path, &choice_id),
                notes: load_group_episode_notes(&episode_media_path),
            }
        })
        .collect();

    let (aggregated, skipped) = aggregate_entity_timeline(&names, &scans);
    let entities: Vec<Value> = aggregated
        .into_iter()
        .map(|entity| {
            let hits: Vec<_> = entity
                .hits
                .iter()
                .map(|hit| {
                    json!({
                        "season": hit.season,
                        "episode": hit.episode,
                        "firstSeenMs": hit.first_seen_ms,
                        "occurrences": hit.occurrences,
                    })
                })
                .collect();
            let notes: Vec<Value> = entity
                .annotations
                .iter()
                .map(|annotation| {
                    json!({
                        "source": "userConfirmedAnnotation",
                        "season": annotation.season,
                        "episode": annotation.episode,
                        "positionMs": annotation.position_ms,
                        "body": annotation.body,
                    })
                })
                .collect();
            json!({
                "name": entity.name,
                "episodeCount": hits.len(),
                "episodes": hits,
                "confirmedAnnotations": notes,
            })
        })
        .collect();
    let skipped: Vec<_> = skipped
        .iter()
        .map(|(season, episode, reason)| {
            json!({ "season": season, "episode": episode, "reason": reason })
        })
        .collect();
    text_result(&json!({
        "derived": true,
        "note": "机械聚合自本机媒体库的台词文本；非语义判断，仅供参考。confirmedAnnotation 为用户确认过的批注，可作为已验证知识。",
        "names": names,
        "entities": entities,
        "skippedEpisodes": skipped,
    }))
}

/// 读取一集的用户确认笔记（批注确认后即为 Note）。读失败按无笔记降级——
/// 语义知识只来自用户确认，AI 没有任何直写路径。
fn load_group_episode_notes(episode_media_path: &Path) -> Vec<(u64, String)> {
    let service =
        lumina_notes::service::NoteService::with_path(lumina_notes::store::default_store_path());
    let Ok(notes) = service.list_for_media(&episode_media_path.to_string_lossy()) else {
        return Vec::new();
    };
    notes
        .into_iter()
        .map(|note| {
            (
                note.position_ms,
                note.body.chars().take(NOTE_BODY_SNIPPET_MAX).collect(),
            )
        })
        .collect()
}

/// 单条笔记正文截断（防长批注拉爆 payload）。
const NOTE_BODY_SNIPPET_MAX: usize = 600;
/// 每个人物最多回带的确认笔记条数。
const ENTITY_TIMELINE_MAX_NOTES: usize = 10;

/// `--lumina-mcp` 子进程不持有播放器：seek 请求写入会话目录的控制文件，
/// 由主 GUI 进程的 watcher 消费并写回执（nonce 匹配 + 限时轮询）。
const SEEK_CONTROL_FILE: &str = "mcp-control.json";
const SEEK_RESULT_FILE: &str = "mcp-control-result.json";
const SEEK_CONFIRM_TIMEOUT_MS: u64 = 8_000;
const SEEK_POLL_INTERVAL_MS: u64 = 200;

fn seek_nonce() -> String {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{unix_ms}-{sequence}")
}

pub(crate) fn seek_control_paths() -> Result<(PathBuf, PathBuf), String> {
    let snapshot_dir = crate::snapshot::resolve_snapshot_path()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .ok_or_else(|| "无法定位会话目录".to_string())?;
    Ok((
        snapshot_dir.join(SEEK_CONTROL_FILE),
        snapshot_dir.join(SEEK_RESULT_FILE),
    ))
}

fn seek_playback(snapshot: &LuminaMcpSnapshot, args: &Value) -> Result<Value, String> {
    // 播放器在主 GUI 进程：本工具把跳转请求写入会话控制文件，由宿主
    // watcher 消费并写回执；这里只做参数校验、下发与限时确认。
    if snapshot
        .anchor
        .as_ref()
        .map(|anchor| anchor.media_path.trim().is_empty())
        .unwrap_or(true)
    {
        return Err("当前没有打开的视频，无法跳转".to_string());
    }

    let position_ms = args
        .get("positionMs")
        .and_then(Value::as_u64)
        .ok_or_else(|| "跳转请求缺少 positionMs".to_string())?;
    let reason = args
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    let (control_path, result_path) = seek_control_paths()?;
    if let Some(parent) = control_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let nonce = seek_nonce();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let request = json!({
        "kind": "seek",
        "positionMs": position_ms,
        "reason": reason,
        "nonce": nonce,
        "requestedAtMs": now_ms,
    });
    fs::write(
        &control_path,
        serde_json::to_string(&request).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("写入跳转请求失败: {error}"))?;

    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(SEEK_CONFIRM_TIMEOUT_MS);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(SEEK_POLL_INTERVAL_MS));
        if let Ok(text) = fs::read_to_string(&result_path) {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                if value.get("nonce").and_then(Value::as_str) == Some(nonce.as_str()) {
                    let status = value
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("error");
                    if status != "ok" {
                        return Err(value
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("播放器未能完成跳转")
                            .to_string());
                    }
                    let landed = value.get("landedPositionMs").and_then(Value::as_u64);
                    return text_result(&json!({
                        "requestedPositionMs": position_ms,
                        "landedPositionMs": landed,
                    }));
                }
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err("播放器未确认跳转请求，请稍后重试".to_string());
        }
    }
}

fn text_result<T: Serialize>(payload: &T) -> Result<Value, String> {
    let text = serde_json::to_string_pretty(payload).map_err(|error| error.to_string())?;
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    }))
}

fn required_i64(args: &Value, key: &str) -> Result<i64, String> {
    let value = args
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("缺少或无效的 {key}"))?;
    if value <= 0 {
        return Err(format!("{key} 必须大于零"));
    }
    Ok(value)
}

fn required_scope(_args: &Value, context: &ChapterTaskContext) -> Result<(i64, i64, i64), String> {
    if context.task_id <= 0 || context.attempt_id <= 0 || context.episode_id <= 0 {
        return Err("当前章节任务范围无效".to_string());
    }
    // The model supplies business fields only. Scope comes from the trusted
    // per-session snapshot, so a model cannot omit, guess, or redirect the
    // task by inventing ids in tool arguments.
    Ok((context.task_id, context.attempt_id, context.episode_id))
}

fn context_file_path(value: &str, label: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || !path.is_file()
    {
        return Err(format!("snapshot 中的 {label} 无效"));
    }
    Ok(path)
}

fn validate_context(context: &ChapterTaskContext) -> Result<(PathBuf, PathBuf), String> {
    if context.task_id <= 0 || context.attempt_id <= 0 || context.episode_id <= 0 {
        return Err("snapshot 中的章节任务标识无效".to_string());
    }
    if context.duration_ms == 0 || context.duration_ms > i64::MAX as u64 {
        return Err("snapshot 中的媒体时长无效".to_string());
    }
    if context.spoiler_boundary.trim().is_empty() || context.spoiler_boundary.len() > 64 {
        return Err("snapshot 中的剧透边界无效".to_string());
    }
    if context.prompt_version.trim().is_empty() || context.prompt_version.len() > 128 {
        return Err("snapshot 中的提示词版本无效".to_string());
    }
    let database = context_file_path(&context.database_path, "数据库位置")?;
    let media = context_file_path(&context.media_path, "媒体位置")?;
    Ok((database, media))
}

fn validate_agent_context(
    repository: &lumina_library::Repository<'_>,
    context: &ChapterTaskContext,
    task_id: i64,
    attempt_id: i64,
    episode_id: i64,
) -> Result<(), String> {
    let task = repository
        .get_agent_task(task_id)
        .map_err(database_message)?
        .ok_or_else(|| "章节任务不存在".to_string())?;
    if !task.task_type.starts_with("chapter")
        || task.episode_id != Some(episode_id)
        || matches!(task.status.as_str(), "succeeded" | "failed")
        || task.prompt_version != context.prompt_version
    {
        return Err("当前章节任务 scope 无效".to_string());
    }
    let attempt = repository
        .get_agent_attempt(attempt_id)
        .map_err(database_message)?
        .ok_or_else(|| "章节任务尝试不存在".to_string())?;
    if attempt.task_id != task_id || attempt.prompt_version != context.prompt_version {
        return Err("任务尝试不属于当前章节任务".to_string());
    }
    Ok(())
}

fn database_message(error: lumina_library::DatabaseError) -> String {
    if let Some(details) = error.details.as_deref() {
        tracing::error!(code = ?error.code, details = %details, "chapter MCP database operation failed");
    }
    error.message
}

fn create_chapter_outline(context: &ChapterTaskContext, args: &Value) -> Result<Value, String> {
    let (database_path, _) = validate_context(context)?;
    let (task_id, attempt_id, episode_id) = required_scope(args, context)?;
    let chapters = args
        .get("chapters")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= 64)
        .ok_or_else(|| "chapters 必须是非空数组".to_string())?;
    let mut parsed = Vec::with_capacity(chapters.len());
    let mut previous_end = 0_u64;
    for item in chapters {
        let stable_id = item
            .get("stableId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= 128)
            .ok_or_else(|| "章节 stableId 无效".to_string())?;
        let start_ms = item
            .get("startMs")
            .and_then(Value::as_u64)
            .ok_or_else(|| "章节 startMs 无效".to_string())?;
        let end_ms = item
            .get("endMs")
            .and_then(Value::as_u64)
            .ok_or_else(|| "章节 endMs 无效".to_string())?;
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= 200)
            .ok_or_else(|| "章节标题无效".to_string())?;
        if start_ms < previous_end || end_ms <= start_ms || end_ms > context.duration_ms {
            return Err("章节时间边界无效或未按顺序排列".to_string());
        }
        previous_end = end_ms;
        parsed.push((stable_id.to_string(), start_ms, end_ms, title.to_string()));
    }

    let mut database = Database::open(database_path).map_err(database_message)?;
    let records = database
        .transaction(|repository| {
            validate_agent_context(repository, context, task_id, attempt_id, episode_id).map_err(
                |message| lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message,
                    details: None,
                },
            )?;
            let records = parsed
                .iter()
                .map(|(stable_id, start_ms, end_ms, title)| {
                    let mut input = NewChapter::new(
                        episode_id,
                        stable_id.clone(),
                        *start_ms as i64,
                        *end_ms as i64,
                        "chapter_agent_mcp",
                    );
                    input.spoiler_level = context.spoiler_boundary.clone();
                    input.title = Some(title.clone());
                    repository.upsert_draft_chapter_for_agent_task(task_id, episode_id, &input)
                })
                .collect::<lumina_library::DatabaseResult<Vec<_>>>()?;
            let keep_ids = records.iter().map(|chapter| chapter.id).collect::<Vec<_>>();
            repository.replace_agent_task_chapter_outline(task_id, episode_id, &keep_ids)?;
            Ok(records)
        })
        .map_err(database_message)?;
    text_result(&json!({
        "taskId": task_id,
        "attemptId": attempt_id,
        "episodeId": episode_id,
        "status": "draft",
        "chapters": records.iter().map(|chapter| json!({
            "chapterId": chapter.id,
            "stableId": chapter.stable_id,
            "startMs": chapter.start_ms,
            "endMs": chapter.end_ms,
            "title": chapter.title,
        })).collect::<Vec<_>>(),
    }))
}

fn capture_chapter_evidence(context: &ChapterTaskContext, args: &Value) -> Result<Value, String> {
    let (database_path, media_path) = validate_context(context)?;
    let (task_id, attempt_id, episode_id) = required_scope(args, context)?;
    let chapter_id = required_i64(args, "chapterId")?;
    let timestamps = args
        .get("timestampsMs")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= 8)
        .ok_or_else(|| "timestampsMs 必须是非空数组".to_string())?;
    let timestamps = timestamps
        .iter()
        .map(|value| value.as_u64().ok_or_else(|| "截图时间无效".to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let mut database = Database::open(&database_path).map_err(database_message)?;
    let chapter = database
        .transaction(|repository| {
            validate_agent_context(repository, context, task_id, attempt_id, episode_id).map_err(
                |message| lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message,
                    details: None,
                },
            )?;
            let chapter = repository.get_chapter(chapter_id)?.ok_or_else(|| {
                lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message: "章节不存在".to_string(),
                    details: None,
                }
            })?;
            if chapter.episode_id != episode_id || chapter.status != "draft" {
                return Err(lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message: "只能为当前任务的 draft 章节采集证据".to_string(),
                    details: None,
                });
            }
            repository.ensure_agent_task_chapter_scope(task_id, episode_id, chapter_id)?;
            Ok(chapter)
        })
        .map_err(database_message)?;
    if timestamps.iter().any(|time| {
        *time > context.duration_ms
            || *time < chapter.start_ms as u64
            || *time > chapter.end_ms as u64
    }) {
        return Err("截图时间必须位于当前章节边界内".to_string());
    }

    let capture_root = database_path
        .parent()
        .ok_or_else(|| "数据库位置缺少父目录".to_string())?
        .join("chapter-assets")
        .join(format!("task-{task_id}"))
        .join(format!("chapter-{chapter_id}"))
        .join(format!("capture-{}-{}", now_ms(), std::process::id()));
    let sample_times = timestamps
        .iter()
        .map(|time| *time as f64 / 1000.0)
        .collect::<Vec<_>>();
    let paths = capture_chapter_evidence_frames(&media_path, &sample_times, &capture_root)?;
    if paths.len() != timestamps.len() {
        return Err("章节画面采集结果数量不一致".to_string());
    }

    let mut database = Database::open(&database_path).map_err(database_message)?;
    let assets = database
        .transaction(|repository| {
            validate_agent_context(repository, context, task_id, attempt_id, episode_id).map_err(
                |message| lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message,
                    details: None,
                },
            )?;
            let mut assets = Vec::with_capacity(paths.len());
            for (path, timestamp) in paths.iter().zip(timestamps.iter()) {
                let bytes = fs::read(path).map_err(|error| lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::QueryFailed,
                    message: "章节画面读取失败".to_string(),
                    details: Some(error.to_string()),
                })?;
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                bytes.hash(&mut hasher);
                let mut input = NewChapterAsset::new(
                    chapter_id,
                    "jpeg_frame",
                    path.to_string_lossy().to_string(),
                    format!("siphash-{:016x}", hasher.finish()),
                    *timestamp as i64,
                    "chapter_agent_mcp",
                );
                input.width = Some(640);
                let asset =
                    repository.insert_chapter_asset_for_agent_task(task_id, episode_id, &input)?;
                assets.push(json!({
                    "assetId": asset.id,
                    "timestampMs": timestamp,
                    "visualContext": format!("本地媒体在 {}ms 的 JPEG 画面证据", timestamp),
                }));
            }
            Ok(assets)
        })
        .map_err(database_message)?;
    let image_bytes = paths
        .iter()
        .map(|path| fs::read(path).map_err(|_| "章节画面读取失败".to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    chapter_evidence_result(
        &json!({
        "taskId": task_id,
        "episodeId": episode_id,
        "chapterId": chapter_id,
        "assets": assets,
        }),
        &image_bytes,
    )
}

/// Test-only seam for the black-box MCP harness.
///
/// The normal path remains the project-local FFmpeg capture implementation.
/// An explicit fixture directory lets a protocol-level test exercise the
/// complete MCP/database path on hosts that do not carry the bundled FFmpeg;
/// the environment variable is never set by the desktop application.
fn capture_chapter_evidence_frames(
    media_path: &Path,
    sample_times: &[f64],
    output_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    #[cfg(feature = "test-support")]
    if let Some(fixture_dir) = std::env::var_os(TEST_CAPTURE_FIXTURE_DIR_ENV) {
        let fixture_dir = PathBuf::from(fixture_dir);
        fs::create_dir_all(output_dir)
            .map_err(|error| format!("测试截图输出目录创建失败: {error}"))?;
        let mut outputs = Vec::with_capacity(sample_times.len());
        for (index, _) in sample_times.iter().enumerate() {
            let candidate = fixture_dir.join(format!("frame-{index:02}.jpg"));
            let fallback = fixture_dir.join("frame.jpg");
            let source = if candidate.is_file() {
                candidate
            } else if fallback.is_file() {
                fallback.clone()
            } else {
                return Err(format!(
                    "测试截图 fixture 缺少 frame-{index:02}.jpg 或 frame.jpg"
                ));
            };
            let output = output_dir.join(format!("frame-{index:02}.jpg"));
            fs::copy(&source, &output)
                .map_err(|error| format!("测试截图 fixture 复制失败: {error}"))?;
            outputs.push(output);
        }
        return Ok(outputs);
    }

    capture_frames(media_path, sample_times, output_dir)
        .map_err(|error| format!("章节画面采集失败: {error}"))
}

fn chapter_evidence_result(metadata: &Value, image_bytes: &[Vec<u8>]) -> Result<Value, String> {
    let text = serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?;
    let mut content = vec![json!({ "type": "text", "text": text })];
    for bytes in image_bytes {
        content.push(json!({
            "type": "image",
            "data": STANDARD.encode(bytes),
            "mimeType": "image/jpeg",
        }));
    }
    Ok(json!({ "content": content, "isError": false }))
}

fn update_chapter_draft(context: &ChapterTaskContext, args: &Value) -> Result<Value, String> {
    let (database_path, _) = validate_context(context)?;
    let (task_id, attempt_id, episode_id) = required_scope(args, context)?;
    let chapter_id = required_i64(args, "chapterId")?;
    let mainline = args
        .get("mainline")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 20_000)
        .ok_or_else(|| "mainline 无效".to_string())?;
    let title = args.get("title").and_then(Value::as_str);
    let draft_key = args
        .get("draftKey")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 128)
        .ok_or_else(|| "draftKey 无效".to_string())?;
    let evidence_ids = args
        .get("evidenceAssetIds")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= 16)
        .ok_or_else(|| "evidenceAssetIds 必须是非空数组".to_string())?
        .iter()
        .map(|value| {
            value
                .as_i64()
                .filter(|id| *id > 0)
                .ok_or_else(|| "证据 assetId 无效".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let recap = args.get("recap").and_then(Value::as_str).unwrap_or("");
    let outlook = args.get("outlook").and_then(Value::as_str).unwrap_or("");
    let highlights = args
        .get("highlights")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let questions = args
        .get("questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if recap.len() > 20_000
        || outlook.len() > 20_000
        || highlights.len() > 32
        || questions.len() > 32
    {
        return Err("章节草稿字段超出限制".to_string());
    }
    let content = json!({
        "title": title,
        "mainline": mainline,
        "recap": recap,
        "outlook": outlook,
        "highlights": highlights,
        "evidenceAssetIds": evidence_ids,
        "draftKey": draft_key,
    });
    let content_text = serde_json::to_string(&content).map_err(|error| error.to_string())?;

    let mut database = Database::open(database_path).map_err(database_message)?;
    let result = database
        .transaction(|repository| {
            validate_agent_context(repository, context, task_id, attempt_id, episode_id).map_err(
                |message| lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message,
                    details: None,
                },
            )?;
            let chapter = repository.get_chapter(chapter_id)?.ok_or_else(|| {
                lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message: "章节不存在".to_string(),
                    details: None,
                }
            })?;
            if chapter.episode_id != episode_id
                || chapter.end_ms > context.duration_ms as i64
                || chapter.status != "draft"
            {
                return Err(lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message: "章节草稿超出当前任务范围".to_string(),
                    details: None,
                });
            }
            let assets = repository.list_chapter_assets_by_chapter(chapter_id)?;
            if evidence_ids
                .iter()
                .any(|id| !assets.iter().any(|asset| asset.id == *id))
            {
                return Err(lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message: "章节证据不属于当前章节".to_string(),
                    details: None,
                });
            }
            repository.ensure_agent_task_chapter_scope(task_id, episode_id, chapter_id)?;
            let chapter = repository.update_draft_chapter_for_agent_task(
                task_id,
                episode_id,
                chapter_id,
                title,
                Some(mainline),
            )?;
            let revision_number = repository
                .get_latest_chapter_revision(chapter_id)?
                .map_or(1, |revision| revision.revision_number + 1);
            let revision = repository.insert_draft_revision_for_agent_task(
                task_id,
                episode_id,
                &NewChapterRevision::new(
                    chapter_id,
                    revision_number,
                    format!("chapter_draft:{draft_key}"),
                    content_text.clone(),
                    "chapter_agent_mcp",
                    context.prompt_version.clone(),
                ),
            )?;
            let mut question_ids = Vec::new();
            let mut question_fingerprints = Vec::new();
            for (index, question) in questions.iter().enumerate() {
                let Some(question) = question.as_str().filter(|value| !value.trim().is_empty())
                else {
                    return Err(lumina_library::DatabaseError {
                        code: lumina_library::DatabaseErrorCode::InvalidInput,
                        message: "问题候选无效".to_string(),
                        details: None,
                    });
                };
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                format!("{task_id}:{chapter_id}:{draft_key}:{index}").hash(&mut hasher);
                let fingerprint = format!("chapter-question-{:016x}", hasher.finish());
                let mut candidate = NewQuestionCandidate::new(
                    question,
                    "chapter_agent_mcp",
                    &context.spoiler_boundary,
                    fingerprint.clone(),
                );
                candidate.episode_id = Some(episode_id);
                candidate.chapter_id = Some(chapter_id);
                candidate.task_id = Some(task_id);
                candidate.batch_key = Some(draft_key.to_string());
                question_fingerprints.push(fingerprint);
                question_ids.push(
                    repository
                        .insert_question_candidate_for_agent_task(task_id, episode_id, &candidate)?
                        .id,
                );
            }
            repository.remove_stale_draft_questions_for_agent_task(
                task_id,
                episode_id,
                chapter_id,
                draft_key,
                &question_fingerprints,
            )?;
            let mut feed_ids = Vec::new();
            let mut feed_parts = vec![("mainline", mainline.to_string())];
            if !recap.trim().is_empty() {
                feed_parts.push(("recap", recap.to_string()));
            }
            if !outlook.trim().is_empty() {
                feed_parts.push(("outlook", outlook.to_string()));
            }
            if !highlights.is_empty() {
                feed_parts.push((
                    "highlights",
                    serde_json::to_string(&highlights).map_err(|error| {
                        lumina_library::DatabaseError {
                            code: lumina_library::DatabaseErrorCode::InvalidInput,
                            message: "章节重点格式无效".to_string(),
                            details: Some(error.to_string()),
                        }
                    })?,
                ));
            }
            let feed_types = feed_parts
                .iter()
                .map(|(kind, _)| (*kind).to_string())
                .collect::<Vec<_>>();
            repository.remove_stale_draft_feed_items_for_agent_task(
                task_id,
                episode_id,
                chapter_id,
                draft_key,
                &feed_types,
            )?;
            for (kind, value) in feed_parts {
                let mut feed = NewWatchFeedItem::new(
                    kind,
                    "chapter_agent_mcp",
                    value,
                    &context.spoiler_boundary,
                    context.prompt_version.clone(),
                    format!("chapter-feed:{task_id}:{chapter_id}:{draft_key}:{kind}"),
                );
                feed.episode_id = Some(episode_id);
                feed.chapter_id = Some(chapter_id);
                feed.revision_id = Some(revision.id);
                feed.task_id = Some(task_id);
                feed_ids.push(
                    repository
                        .insert_draft_feed_item_for_agent_task(task_id, episode_id, &feed)?
                        .id,
                );
            }
            Ok(json!({
                "chapterId": chapter.id,
                "revisionId": revision.id,
                "questionIds": question_ids,
                "feedDraftIds": feed_ids,
                "status": "draft",
            }))
        })
        .map_err(database_message)?;
    text_result(&result)
}

fn finalize_chapter_task(context: &ChapterTaskContext, args: &Value) -> Result<Value, String> {
    let (database_path, _) = validate_context(context)?;
    let (task_id, attempt_id, episode_id) = required_scope(args, context)?;
    let mut database = Database::open(database_path).map_err(database_message)?;
    let result = database
        .transaction(|repository| {
            validate_agent_context(repository, context, task_id, attempt_id, episode_id).map_err(
                |message| lumina_library::DatabaseError {
                    code: lumina_library::DatabaseErrorCode::InvalidInput,
                    message,
                    details: None,
                },
            )?;
            let output = serde_json::to_string(&json!({
                "taskId": task_id,
                "attemptId": attempt_id,
                "episodeId": episode_id,
                "status": "published",
            }))
            .map_err(|error| lumina_library::DatabaseError {
                code: lumina_library::DatabaseErrorCode::InvalidInput,
                message: "章节任务输出格式无效".to_string(),
                details: Some(error.to_string()),
            })?;
            let chapters = repository.publish_agent_chapter_task(
                task_id,
                attempt_id,
                episode_id,
                context.duration_ms as i64,
                &output,
            )?;
            Ok(json!({
                "taskId": task_id,
                "episodeId": episode_id,
                "status": "published",
                "chapterIds": chapters.iter().map(|chapter| chapter.id).collect::<Vec<_>>(),
            }))
        })
        .map_err(database_message)?;
    text_result(&result)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{AgentCapabilities, ChapterTaskContext, CONTEXT_FILE_ENV};
    use std::sync::{Mutex, OnceLock};

    static CONTEXT_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn context_env_lock() -> std::sync::MutexGuard<'static, ()> {
        CONTEXT_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn seek_snapshot() -> LuminaMcpSnapshot {
        LuminaMcpSnapshot {
            anchor: Some(PromptAnchor {
                media_path: r"D:\videos\demo.mp4".into(),
                media_title: None,
                library_root: None,
                group_key: None,
                season: None,
                episode: None,
                position_ms: 10_000,
                duration_ms: Some(60_000),
                sent_at_ms: 0,
                subtitle_choice_id: None,
                transcript_window_radius_sec: None,
            }),
            ..LuminaMcpSnapshot::empty()
        }
    }

    #[test]
    fn seek_tool_refuses_without_anchor() {
        let snapshot = LuminaMcpSnapshot::empty();
        let err = seek_playback(&snapshot, &json!({ "positionMs": 1_000 }))
            .expect_err("no anchor must refuse");
        assert!(err.contains("没有打开的视频"));
    }

    #[test]
    fn entity_timeline_aggregates_hits_and_skips_failed_transcripts() {
        let episodes = vec![
            EpisodeScanInput {
                season: 1,
                episode: 1,
                media_file_name: "S01E01.mkv".into(),
                transcript: Err("无字幕".into()),
                notes: Vec::new(),
            },
            EpisodeScanInput {
                notes: vec![(30_000, "焕金与崔雄的关系线：重逢后的和解".into())],
                ..seek_scan_helper(
                    1,
                    2,
                    &[("焕金说了一句台词", 1_000), ("焕金与崔雄再次相见", 5_000)],
                )
            },
        ];
        let (aggregated, skipped) =
            aggregate_entity_timeline(&["焕金".to_string(), "不存在角色".to_string()], &episodes);
        assert_eq!(aggregated[0].name, "焕金");
        assert_eq!(aggregated[0].hits.len(), 1);
        assert_eq!(aggregated[0].hits[0].season, 1);
        assert_eq!(aggregated[0].hits[0].episode, 2);
        assert_eq!(aggregated[0].hits[0].first_seen_ms, 1_000);
        assert_eq!(aggregated[0].hits[0].occurrences, 2);
        // 用户确认批注与台词命中独立：正文提到名字即回带。
        assert_eq!(aggregated[0].annotations.len(), 1);
        assert_eq!(aggregated[0].annotations[0].episode, 2);
        assert_eq!(aggregated[0].annotations[0].position_ms, 30_000);
        assert!(aggregated[0].annotations[0].body.contains("崔雄"));
        // 无命中的名字也原样返回（0 命中即「没有出现」，不编造）。
        assert!(aggregated[1].hits.is_empty());
        assert!(aggregated[1].annotations.is_empty());
        assert_eq!(skipped, vec![(1, 1, "无字幕".to_string())]);
    }

    fn seek_scan_helper(season: u32, episode: u32, cues: &[(&str, u64)]) -> EpisodeScanInput {
        let transcript = Transcript {
            source_path: format!("S{:02}E{:02}.mkv", season, episode),
            choice_id: "embedded:0".into(),
            stream_index: None,
            language: None,
            codec_name: None,
            cues: cues
                .iter()
                .map(|(text, start)| Cue {
                    index: 0,
                    start_ms: *start,
                    end_ms: *start + 1,
                    text: (*text).into(),
                })
                .collect(),
        };
        EpisodeScanInput {
            season,
            episode,
            media_file_name: format!("S{:02}E{:02}.mkv", season, episode),
            transcript: Ok(transcript),
            notes: Vec::new(),
        }
    }

    #[test]
    fn entity_timeline_requires_names() {
        let snapshot = seek_snapshot();
        let err = entity_timeline(&snapshot, &json!({})).expect_err("names required");
        assert!(err.contains("names"));
    }

    #[test]
    fn seek_tool_requires_position() {
        let err = seek_playback(&seek_snapshot(), &json!({}))
            .expect_err("missing positionMs must refuse");
        assert!(err.contains("positionMs"));
    }

    /// Full round trip: tool writes the control file, a fake host watcher
    /// confirms with the same nonce, and the tool reports the landed position.
    /// Env var is restored afterwards (same convention as the capture chain).
    #[test]
    fn seek_tool_round_trips_through_control_file() {
        let _context_guard = context_env_lock();
        let dir = std::env::temp_dir().join(format!("lumina-seek-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("seek temp dir");
        let previous = std::env::var(CONTEXT_FILE_ENV).ok();
        std::env::set_var(
            CONTEXT_FILE_ENV,
            dir.join(".lumina").join("agent-context.json"),
        );

        let seek_dir = dir.clone();
        let fake_host = std::thread::spawn(move || {
            let control = seek_dir.join(".lumina").join("mcp-control.json");
            let mut seen = String::new();
            for _ in 0..40 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if let Ok(text) = std::fs::read_to_string(&control) {
                    if text != seen {
                        seen = text;
                        break;
                    }
                }
            }
            assert!(!seen.is_empty(), "control file never appeared");
            let value: Value = serde_json::from_str(&seen).expect("control json");
            let nonce = value.get("nonce").and_then(Value::as_str).expect("nonce");
            assert_eq!(
                value.get("positionMs").and_then(Value::as_u64),
                Some(42_000)
            );
            std::fs::write(
                seek_dir.join(".lumina").join("mcp-control-result.json"),
                serde_json::to_string(&json!({
                    "nonce": nonce,
                    "status": "ok",
                    "landedPositionMs": 42_000,
                }))
                .expect("result json"),
            )
            .expect("write result");
        });

        let result = seek_playback(&seek_snapshot(), &json!({ "positionMs": 42_000 }))
            .expect("seek round trip");
        let _ = fake_host.join();
        let text = result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .expect("text payload");
        assert!(text.contains("42000"), "landed position reported: {text}");

        match previous {
            Some(value) => std::env::set_var(CONTEXT_FILE_ENV, value),
            None => std::env::remove_var(CONTEXT_FILE_ENV),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

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
        let _context_guard = context_env_lock();
        let dir = std::env::temp_dir().join(format!("lumina-capture-chain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("chain temp dir");
        let media = dir.join("chain-20s.mp4");
        if !create_synthetic_video(&media, 20) {
            eprintln!("SKIP capture chain: ffmpeg not vendored on this machine");
            return;
        }

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
                transcript_window_radius_sec: None,
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
        let _context_guard = context_env_lock();
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
                duration_ms: Some(4_000),
                sent_at_ms: 9,
                subtitle_choice_id: None,
                transcript_window_radius_sec: None,
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
    fn transcript_window_uses_anchor_preference_when_window_is_omitted() {
        let mut snapshot = online_snapshot_with_transcript();
        snapshot
            .anchor
            .as_mut()
            .expect("anchor")
            .transcript_window_radius_sec = Some(15);

        let value = transcript_window(&snapshot, &json!({})).expect("preferred window");
        let payload = tool_text_payload(&value);
        assert_eq!(payload.get("beforeSec").and_then(Value::as_u64), Some(15));
        assert_eq!(payload.get("afterSec").and_then(Value::as_u64), Some(15));
    }

    #[test]
    fn transcript_window_explicit_args_override_anchor_preference() {
        let mut snapshot = online_snapshot_with_transcript();
        snapshot
            .anchor
            .as_mut()
            .expect("anchor")
            .transcript_window_radius_sec = Some(15);

        let value =
            transcript_window(&snapshot, &json!({ "radiusSec": 3 })).expect("explicit window");
        let payload = tool_text_payload(&value);
        assert_eq!(payload.get("beforeSec").and_then(Value::as_u64), Some(3));
        assert_eq!(payload.get("afterSec").and_then(Value::as_u64), Some(3));
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
            transcript_window_radius_sec: None,
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
                transcript_window_radius_sec: None,
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
                    transcript_window_radius_sec: None,
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
                transcript_window_radius_sec: None,
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

    #[test]
    fn chapter_evidence_result_exposes_mcp_images_without_local_paths() {
        let metadata = json!({
            "assets": [{
                "assetId": 42,
                "timestampMs": 1200,
                "visualContext": "本地媒体在 1200ms 的 JPEG 画面证据"
            }]
        });
        let result =
            chapter_evidence_result(&metadata, &[vec![0xff, 0xd8, 0xff]]).expect("evidence result");
        let blocks = result
            .get("content")
            .and_then(Value::as_array)
            .expect("content blocks");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].get("type").and_then(Value::as_str), Some("image"));
        assert_eq!(
            blocks[1].get("mimeType").and_then(Value::as_str),
            Some("image/jpeg")
        );
        assert!(blocks[1].get("data").and_then(Value::as_str).is_some());
        let serialized = serde_json::to_string(&result).expect("serialize result");
        assert!(!serialized.contains("chapter-assets"));
        assert!(!serialized.contains(".jpg"));
    }

    struct ChapterToolFixture {
        dir: PathBuf,
        context: ChapterTaskContext,
        task_id: i64,
        attempt_id: i64,
        episode_id: i64,
    }

    impl Drop for ChapterToolFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn chapter_tool_fixture() -> ChapterToolFixture {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "lumina-mcp-chapter-tools-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("fixture directory");
        let database_path = dir.join("lumina.sqlite3");
        let media_path = dir.join("episode.mkv");
        fs::write(&media_path, b"dummy media").expect("dummy media");

        let (task_id, attempt_id, episode_id) = {
            let database = Database::open(&database_path).expect("fixture database");
            let repository = database.repository();
            let series_id = repository
                .insert_series(&lumina_library::NewSeries::new(
                    "fixture-series",
                    "Fixture Series",
                    "test",
                ))
                .expect("fixture series");
            let mut episode = lumina_library::NewEpisode::new(series_id, "fixture-episode", "test");
            episode.duration_ms = Some(10_000);
            let episode_id = repository
                .insert_episode(&episode)
                .expect("fixture episode");
            let mut task = lumina_library::NewAgentTask::new(
                "fixture-chapter-task",
                "chapter_generation",
                "chapter-prompt-v1",
            );
            task.episode_id = Some(episode_id);
            task.status = "running".into();
            let task_id = repository.insert_agent_task(&task).expect("fixture task");
            let attempt_id = repository
                .insert_agent_attempt(&lumina_library::NewAgentAttempt::new(
                    task_id,
                    1,
                    "initial",
                    "running",
                    "chapter-prompt-v1",
                    1,
                ))
                .expect("fixture attempt");
            (task_id, attempt_id, episode_id)
        };

        ChapterToolFixture {
            dir,
            context: ChapterTaskContext {
                task_id,
                attempt_id,
                episode_id,
                database_path: database_path.to_string_lossy().into_owned(),
                media_path: media_path.to_string_lossy().into_owned(),
                duration_ms: 10_000,
                spoiler_boundary: "episode".into(),
                prompt_version: "chapter-prompt-v1".into(),
            },
            task_id,
            attempt_id,
            episode_id,
        }
    }

    fn chapter_tool_payload(value: &Value) -> Value {
        let text = value
            .get("content")
            .and_then(Value::as_array)
            .and_then(|blocks| blocks.first())
            .and_then(|block| block.get("text"))
            .and_then(Value::as_str)
            .expect("chapter tool text result");
        serde_json::from_str(text).expect("chapter tool JSON payload")
    }

    fn invoke_chapter_tool(
        fixture: &ChapterToolFixture,
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        invoke_chapter_tool_with_context(&fixture.context, name, args)
    }

    fn invoke_chapter_tool_with_context(
        context: &ChapterTaskContext,
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        handle_tool_call_with_context(&LuminaMcpSnapshot::empty(), Some(context), name, &args)
    }

    fn create_synthetic_video(path: &Path, duration_sec: u32) -> bool {
        let ffmpeg = match lumina_media::tools::resolve_ffmpeg() {
            Ok(path) => path,
            Err(_) => return false,
        };
        let status = lumina_media::process::command(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                &format!("testsrc=duration={duration_sec}:size=640x360:rate=30"),
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-an",
            ])
            .arg(path)
            .output()
            .expect("spawn ffmpeg");
        assert!(
            status.status.success(),
            "synthetic fixture failed to encode"
        );
        true
    }

    fn chapter_scope_args(_fixture: &ChapterToolFixture) -> Value {
        json!({})
    }

    #[test]
    fn direct_capture_handler_persists_asset_and_returns_mcp_image() {
        let fixture = chapter_tool_fixture();
        let media_path = fixture.dir.join("capture-2s.mp4");
        if !create_synthetic_video(&media_path, 2) {
            eprintln!("SKIP chapter evidence capture: ffmpeg not vendored on this machine");
            return;
        }
        let context = ChapterTaskContext {
            media_path: media_path.to_string_lossy().into_owned(),
            ..fixture.context.clone()
        };

        let mut outline_args = chapter_scope_args(&fixture);
        outline_args["chapters"] = json!([{
            "stableId": "scene-capture",
            "startMs": 0,
            "endMs": 1_500,
            "title": "Captured scene"
        }]);
        let outline = invoke_chapter_tool_with_context(
            &context,
            lumina_core::tool_contract::TOOL_CREATE_CHAPTER_OUTLINE,
            outline_args,
        )
        .expect("outline");
        let chapter_id = chapter_tool_payload(&outline)["chapters"][0]["chapterId"]
            .as_i64()
            .expect("chapter id");

        let mut capture_args = chapter_scope_args(&fixture);
        capture_args["chapterId"] = json!(chapter_id);
        capture_args["timestampsMs"] = json!([1_000]);
        let result = invoke_chapter_tool_with_context(
            &context,
            lumina_core::tool_contract::TOOL_CAPTURE_CHAPTER_EVIDENCE,
            capture_args,
        )
        .expect("chapter evidence capture");
        let blocks = result
            .get("content")
            .and_then(Value::as_array)
            .expect("MCP content blocks");
        let image = blocks
            .iter()
            .find(|block| block.get("type").and_then(Value::as_str) == Some("image"))
            .expect("MCP image block");
        assert_eq!(
            image.get("mimeType").and_then(Value::as_str),
            Some("image/jpeg")
        );
        assert!(image
            .get("data")
            .and_then(Value::as_str)
            .is_some_and(|data| data.starts_with("/9j/")));
        let serialized = serde_json::to_string(&result).expect("serialize MCP result");
        assert!(!serialized.contains("chapter-assets"));
        assert!(!serialized.contains("capture-2s.mp4"));

        let database = Database::open(&fixture.context.database_path).expect("reopen fixture db");
        let assets = database
            .repository()
            .list_chapter_assets_by_chapter(chapter_id)
            .expect("chapter assets");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].asset_type, "jpeg_frame");
        assert_eq!(assets[0].source, "chapter_agent_mcp");
        assert!(Path::new(&assets[0].path).is_file());
    }

    #[test]
    fn direct_outline_handler_persists_idempotent_stable_chapter_mapping() {
        let fixture = chapter_tool_fixture();
        let mut args = chapter_scope_args(&fixture);
        args["chapters"] = json!([{
            "stableId": "scene-001",
            "startMs": 0,
            "endMs": 4_000,
            "title": "Opening"
        }]);

        let first = invoke_chapter_tool(
            &fixture,
            lumina_core::tool_contract::TOOL_CREATE_CHAPTER_OUTLINE,
            args.clone(),
        )
        .expect("first outline");
        let first_payload = chapter_tool_payload(&first);
        let first_id = first_payload["chapters"][0]["chapterId"]
            .as_i64()
            .expect("first chapter id");
        let second = invoke_chapter_tool(
            &fixture,
            lumina_core::tool_contract::TOOL_CREATE_CHAPTER_OUTLINE,
            args,
        )
        .expect("idempotent outline");
        let second_id = chapter_tool_payload(&second)["chapters"][0]["chapterId"]
            .as_i64()
            .expect("second chapter id");
        assert_eq!(first_id, second_id);

        let database = Database::open(&fixture.context.database_path).expect("reopen fixture db");
        let chapters = database
            .repository()
            .list_chapters_by_agent_task(fixture.task_id, fixture.episode_id)
            .expect("task chapters");
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].id, first_id);
        assert_eq!(chapters[0].stable_id, "scene-001");
    }

    #[test]
    fn direct_update_asset_and_finalize_handlers_persist_and_publish_atomically() {
        let fixture = chapter_tool_fixture();
        let mut outline_args = chapter_scope_args(&fixture);
        outline_args["chapters"] = json!([{
            "stableId": "scene-final",
            "startMs": 0,
            "endMs": 5_000,
            "title": "Final scene"
        }]);
        let outline = invoke_chapter_tool(
            &fixture,
            lumina_core::tool_contract::TOOL_CREATE_CHAPTER_OUTLINE,
            outline_args,
        )
        .expect("outline");
        let chapter_id = chapter_tool_payload(&outline)["chapters"][0]["chapterId"]
            .as_i64()
            .expect("chapter id");

        let asset_path = fixture.dir.join("durable-frame.jpg");
        fs::write(&asset_path, b"jpeg fixture").expect("fixture asset");
        let database = Database::open(&fixture.context.database_path).expect("open fixture db");
        let asset = database
            .repository()
            .insert_chapter_asset(&lumina_library::NewChapterAsset::new(
                chapter_id,
                "jpeg_frame",
                asset_path.to_string_lossy().into_owned(),
                "fixture-hash",
                1_000,
                "test",
            ))
            .expect("persistent asset");
        drop(database);

        let mut update_args = chapter_scope_args(&fixture);
        update_args["chapterId"] = json!(chapter_id);
        update_args["mainline"] = json!("The durable chapter draft.");
        update_args["recap"] = json!("A short recap.");
        update_args["questions"] = json!(["What changes next?"]);
        update_args["evidenceAssetIds"] = json!([asset]);
        update_args["draftKey"] = json!("draft-001");
        let update = invoke_chapter_tool(
            &fixture,
            lumina_core::tool_contract::TOOL_UPDATE_CHAPTER_DRAFT,
            update_args,
        )
        .expect("chapter update");
        assert_eq!(chapter_tool_payload(&update)["status"], "draft");

        let finalize = invoke_chapter_tool(
            &fixture,
            lumina_core::tool_contract::TOOL_FINALIZE_CHAPTER_TASK,
            chapter_scope_args(&fixture),
        )
        .expect("chapter finalize");
        assert_eq!(chapter_tool_payload(&finalize)["status"], "published");

        let database = Database::open(&fixture.context.database_path).expect("reopen published db");
        let repository = database.repository();
        assert_eq!(
            repository
                .get_agent_task(fixture.task_id)
                .expect("task")
                .expect("task row")
                .status,
            "succeeded"
        );
        assert_eq!(
            repository
                .get_agent_attempt(fixture.attempt_id)
                .expect("attempt")
                .expect("attempt row")
                .status,
            "succeeded"
        );
        assert_eq!(
            repository
                .get_chapter(chapter_id)
                .expect("chapter")
                .expect("chapter row")
                .status,
            "ready"
        );
        assert_eq!(
            repository
                .get_latest_chapter_revision(chapter_id)
                .expect("revision")
                .expect("revision row")
                .status,
            "accepted"
        );
        assert!(repository
            .list_watch_feed_items_by_episode(fixture.episode_id)
            .expect("feed")
            .iter()
            .any(|item| item.chapter_id == Some(chapter_id) && item.published_at_ms.is_some()));
    }

    #[test]
    fn direct_chapter_handlers_reject_cross_task_and_cross_episode_scope() {
        let fixture = chapter_tool_fixture();
        let mut cross_task = chapter_scope_args(&fixture);
        cross_task["chapters"] = json!([{
            "stableId": "cross-task",
            "startMs": 0,
            "endMs": 1_000,
            "title": "Rejected"
        }]);
        let cross_task_context = ChapterTaskContext {
            task_id: fixture.task_id + 1,
            ..fixture.context.clone()
        };
        let error = invoke_chapter_tool_with_context(
            &cross_task_context,
            lumina_core::tool_contract::TOOL_CREATE_CHAPTER_OUTLINE,
            cross_task,
        )
        .expect_err("cross-task outline must be rejected");
        assert_eq!(error, "章节任务不存在");

        let mut cross_episode = chapter_scope_args(&fixture);
        cross_episode["chapters"] = json!([{
            "stableId": "cross-episode",
            "startMs": 0,
            "endMs": 1_000,
            "title": "Rejected"
        }]);
        let cross_episode_context = ChapterTaskContext {
            episode_id: fixture.episode_id + 1,
            ..fixture.context.clone()
        };
        let error = invoke_chapter_tool_with_context(
            &cross_episode_context,
            lumina_core::tool_contract::TOOL_CREATE_CHAPTER_OUTLINE,
            cross_episode,
        )
        .expect_err("cross-episode outline must be rejected");
        assert_eq!(error, "当前章节任务 scope 无效");
    }
}
