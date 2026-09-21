//! Canonical agent-tool contract, transport-free (Step 8).
//!
//! Identity, argument bounds, and stable result/error boundaries for Lumina's
//! agent tools. Deliberately dependency-free (`std` only): no MCP transport,
//! no JSON-RPC types, no snapshot IO, and no Cookie / signed-URL / absolute
//! path / CLI / stderr material. User-facing wording stays owned by each
//! domain (see [`crate::media_source`]); this module only fixes machine
//! boundaries so a future non-MCP caller gets byte-identical behavior.
//!
//! MCP (`lumina-mcp`) consumes this module for dispatch, arg clamping, and
//! `tools/list` parity. Transport envelopes, `ToolPolicy`, snapshot reads,
//! and domain service calls stay in MCP.

/// Tool identities (frozen; `tools/list`, `tools/call`, and policy share them).
pub const TOOL_PLAYBACK_CONTEXT: &str = "lumina_get_playback_context";
pub const TOOL_LIBRARY_CONTEXT: &str = "lumina_get_library_context";
pub const TOOL_EPISODE_INDEX: &str = "lumina_get_episode_index";
pub const TOOL_TRANSCRIPT_WINDOW: &str = "lumina_get_transcript_window";
pub const TOOL_EPISODE_TRANSCRIPT: &str = "lumina_get_episode_transcript";
pub const TOOL_AUDIO_MARKS: &str = "lumina_get_audio_marks";
pub const TOOL_SUBTITLE_CUES: &str = "lumina_get_subtitle_cues";
pub const TOOL_WRITE_SUBTITLE_TRACK: &str = "lumina_write_subtitle_track";
pub const TOOL_CAPTURE_FRAMES: &str = "lumina_capture_frames";
pub const TOOL_PROPOSE_ANNOTATION: &str = "lumina_propose_video_annotation";
pub const TOOL_SEEK_PLAYBACK: &str = "lumina_seek_playback";
pub const TOOL_ENTITY_TIMELINE: &str = "lumina_get_entity_timeline";
pub const TOOL_CREATE_CHAPTER_OUTLINE: &str = "lumina_create_chapter_outline";
pub const TOOL_CAPTURE_CHAPTER_EVIDENCE: &str = "lumina_capture_chapter_evidence";
pub const TOOL_UPDATE_CHAPTER_DRAFT: &str = "lumina_update_chapter_draft";
pub const TOOL_FINALIZE_CHAPTER_TASK: &str = "lumina_finalize_chapter_task";

/// Every known tool, in `tools/list` declaration order.
pub const ALL_TOOLS: &[&str] = &[
    TOOL_PLAYBACK_CONTEXT,
    TOOL_LIBRARY_CONTEXT,
    TOOL_EPISODE_INDEX,
    TOOL_TRANSCRIPT_WINDOW,
    TOOL_EPISODE_TRANSCRIPT,
    TOOL_AUDIO_MARKS,
    TOOL_SUBTITLE_CUES,
    TOOL_WRITE_SUBTITLE_TRACK,
    TOOL_CAPTURE_FRAMES,
    TOOL_PROPOSE_ANNOTATION,
    TOOL_SEEK_PLAYBACK,
    TOOL_ENTITY_TIMELINE,
    TOOL_CREATE_CHAPTER_OUTLINE,
    TOOL_CAPTURE_CHAPTER_EVIDENCE,
    TOOL_UPDATE_CHAPTER_DRAFT,
    TOOL_FINALIZE_CHAPTER_TASK,
];

/// Number of known tools (locked: adding a tool is a contract change).
pub const TOOL_COUNT: usize = 16;

/// True for the canonical names, false for anything else (including the
/// historical `Unknown tool` path, which MCP preserves verbatim).
pub fn is_known_tool(name: &str) -> bool {
    matches!(
        name,
        TOOL_PLAYBACK_CONTEXT
            | TOOL_LIBRARY_CONTEXT
            | TOOL_EPISODE_INDEX
            | TOOL_TRANSCRIPT_WINDOW
            | TOOL_EPISODE_TRANSCRIPT
            | TOOL_AUDIO_MARKS
            | TOOL_SUBTITLE_CUES
            | TOOL_WRITE_SUBTITLE_TRACK
            | TOOL_CAPTURE_FRAMES
            | TOOL_PROPOSE_ANNOTATION
            | TOOL_SEEK_PLAYBACK
            | TOOL_ENTITY_TIMELINE
            | TOOL_CREATE_CHAPTER_OUTLINE
            | TOOL_CAPTURE_CHAPTER_EVIDENCE
            | TOOL_UPDATE_CHAPTER_DRAFT
            | TOOL_FINALIZE_CHAPTER_TASK
    )
}

// --- Argument bounds (frozen; mirror the MCP `inputSchema` limits) ---

/// Transcript/audio window default (each side, seconds).
pub const WINDOW_DEFAULT_BEFORE_SEC: u32 = 60;
/// Transcript/audio window default (each side, seconds).
pub const WINDOW_DEFAULT_AFTER_SEC: u32 = 60;
/// Transcript/audio window cap (each side, seconds).
pub const WINDOW_MAX_SEC: u32 = 300;

/// Capture window default (each side, seconds; single frame at center).
pub const CAPTURE_DEFAULT_BEFORE_SEC: u32 = 0;
/// Capture window default (each side, seconds).
pub const CAPTURE_DEFAULT_AFTER_SEC: u32 = 0;
/// Capture window cap (each side, seconds).
pub const CAPTURE_MAX_SEC: u32 = 7;

/// Subtitle-cues paging default page size.
pub const SUBTITLE_CUES_DEFAULT_LIMIT: usize = 80;
/// Subtitle-cues paging cap.
pub const SUBTITLE_CUES_MAX_LIMIT: usize = 200;

/// Write-track batch cap (cues per call).
pub const WRITE_CUES_MAX: usize = 2000;
/// Write-track `lang` token schema bounds (`minLength`/`maxLength`).
pub const WRITE_LANG_MIN_LEN: usize = 1;
/// Write-track `lang` token schema bounds.
pub const WRITE_LANG_MAX_LEN: usize = 24;

/// `season`/`episode` lower bound (schema `minimum: 1`).
pub const SEASON_EPISODE_MIN: u32 = 1;

/// Scene-detection threshold clamp range.
pub const SCENE_THRESHOLD_MIN: f32 = 0.1;
/// Scene-detection threshold clamp range.
pub const SCENE_THRESHOLD_MAX: f32 = 0.9;

// --- Pure argument resolution (no transport types) ---

/// Resolve `(before_sec, after_sec)` from optional `radiusSec`/`beforeSec`/
/// `afterSec` (already extracted from transport args). `radiusSec` wins and
/// mirrors to both sides; each side caps at [`WINDOW_MAX_SEC`].
pub fn resolve_window(
    before_sec: Option<u64>,
    after_sec: Option<u64>,
    radius_sec: Option<u64>,
    default_before: u32,
    default_after: u32,
) -> (u32, u32) {
    if let Some(radius) = radius_sec {
        let radius = radius.min(u64::from(WINDOW_MAX_SEC)) as u32;
        return (radius, radius);
    }
    let before = before_sec
        .map(|value| value.min(u64::from(WINDOW_MAX_SEC)) as u32)
        .unwrap_or(default_before);
    let after = after_sec
        .map(|value| value.min(u64::from(WINDOW_MAX_SEC)) as u32)
        .unwrap_or(default_after);
    (before, after)
}

/// Resolve the capture window (defaults single-frame, each side capped at
/// [`CAPTURE_MAX_SEC`]).
pub fn resolve_capture_window(
    before_sec: Option<u64>,
    after_sec: Option<u64>,
    radius_sec: Option<u64>,
) -> (u32, u32) {
    let (before, after) = resolve_window(
        before_sec,
        after_sec,
        radius_sec,
        CAPTURE_DEFAULT_BEFORE_SEC,
        CAPTURE_DEFAULT_AFTER_SEC,
    );
    (before.min(CAPTURE_MAX_SEC), after.min(CAPTURE_MAX_SEC))
}

/// Resolve the time center: explicit `centerMs` wins, else `atSec * 1000`,
/// else the frozen anchor; clamped down to `duration_ms` when known.
pub fn resolve_center_ms(
    center_ms: Option<u64>,
    at_sec: Option<u64>,
    default_center_ms: u64,
    duration_ms: Option<u64>,
) -> u64 {
    let center = if let Some(ms) = center_ms {
        ms
    } else if let Some(sec) = at_sec {
        sec.saturating_mul(1000)
    } else {
        default_center_ms
    };
    duration_ms.map_or(center, |duration| center.min(duration))
}

/// Clamp a cues page offset (missing means `0`).
pub fn clamp_cues_offset(offset: Option<u64>) -> usize {
    offset.unwrap_or(0) as usize
}

/// Clamp a cues page limit (missing means [`SUBTITLE_CUES_DEFAULT_LIMIT`],
/// capped at [`SUBTITLE_CUES_MAX_LIMIT`]).
pub fn clamp_cues_limit(limit: Option<u64>) -> usize {
    limit
        .map(|value| value.min(SUBTITLE_CUES_MAX_LIMIT as u64) as usize)
        .unwrap_or(SUBTITLE_CUES_DEFAULT_LIMIT)
}

/// Slice a cue list into one page. Returns `(start, count, has_more)` where
/// `start`/`count` index the caller's slice. Never panics on `offset` past
/// the end (yields an empty page with `has_more == false`).
pub fn paginate(total: usize, offset: usize, limit: usize) -> (usize, usize, bool) {
    let start = offset.min(total);
    let count = limit.min(total.saturating_sub(start));
    let has_more = start.saturating_add(count) < total;
    (start, count, has_more)
}

/// Machine-readable `season`/`episode` validation. Domains map each variant
/// to their own fixed wording (MCP keeps `缺少 season` / `缺少 episode` /
/// `season 与 episode 须大于 0` byte-identical).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonEpisodeError {
    MissingSeason,
    MissingEpisode,
    NonPositive,
}

/// Validate required `season` + `episode` (schema `minimum: 1`).
pub fn validate_season_episode(
    season: Option<u64>,
    episode: Option<u64>,
) -> Result<(u32, u32), SeasonEpisodeError> {
    let season = season.ok_or(SeasonEpisodeError::MissingSeason)? as u32;
    let episode = episode.ok_or(SeasonEpisodeError::MissingEpisode)? as u32;
    if season < SEASON_EPISODE_MIN || episode < SEASON_EPISODE_MIN {
        return Err(SeasonEpisodeError::NonPositive);
    }
    Ok((season, episode))
}

/// Machine-readable write-batch size validation. Domains keep their own
/// wording (`cues 不能为空` / `单次写入字幕过多，请分批` in MCP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteCuesError {
    Empty,
    TooMany,
}

/// Validate a write-track batch length (schema `minItems: 1`, cap
/// [`WRITE_CUES_MAX`]).
pub fn validate_write_cues_len(len: usize) -> Result<(), WriteCuesError> {
    if len == 0 {
        return Err(WriteCuesError::Empty);
    }
    if len > WRITE_CUES_MAX {
        return Err(WriteCuesError::TooMany);
    }
    Ok(())
}

/// Opt-in scene sampling: only `"scene"` (any case) enables it.
pub fn is_scene_mode(mode: Option<&str>) -> bool {
    mode.is_some_and(|mode| mode.eq_ignore_ascii_case("scene"))
}

/// Clamp a scene threshold into [`SCENE_THRESHOLD_MIN`]..=[`SCENE_THRESHOLD_MAX`];
/// missing/invalid callers pass `None` and get `default`.
pub fn clamp_scene_threshold(value: Option<f64>, default: f32) -> f32 {
    value
        .map(|value| (value as f32).clamp(SCENE_THRESHOLD_MIN, SCENE_THRESHOLD_MAX))
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn tool_identities_are_frozen() {
        assert_eq!(ALL_TOOLS.len(), TOOL_COUNT);
        let set: HashSet<&str> = ALL_TOOLS.iter().copied().collect();
        assert_eq!(set.len(), TOOL_COUNT, "tool names must be unique");
        for name in ALL_TOOLS {
            assert!(!name.trim().is_empty());
            assert!(
                name.starts_with("lumina_"),
                "tool name must keep the lumina_ prefix: {name}"
            );
            assert!(is_known_tool(name));
        }
        assert!(!is_known_tool("lumina_do_anything"));
        assert!(!is_known_tool(""));
    }

    #[test]
    fn contract_carries_no_transport_or_secret_material() {
        // Tripwire: names and bounds must never smuggle Cookie / signed-URL /
        // path / CLI / stderr / JSON-RPC vocabulary into the shared contract.
        let mut text = ALL_TOOLS.join("\n").to_lowercase();
        text.push_str(&format!(
            "{WINDOW_MAX_SEC}{CAPTURE_MAX_SEC}{SUBTITLE_CUES_MAX_LIMIT}{WRITE_CUES_MAX}"
        ));
        for banned in [
            "cookie",
            "sig=",
            "signed",
            "stderr",
            "yt-dlp",
            "ffmpeg",
            "mpv-cookies",
            "cookies.txt",
            "--cookies",
            "jsonrpc",
            "json-rpc",
            "serde_json",
            "tauri",
            "libmpv",
        ] {
            assert!(!text.contains(banned), "banned {banned} in contract");
        }
    }

    #[test]
    fn core_manifest_has_no_reverse_dependencies() {
        let manifest = include_str!("../Cargo.toml").to_lowercase();
        for banned in [
            "lumina-mcp",
            "lumina-acp",
            "lumina-library",
            "lumina-media",
            "lumina-player",
            "lumina-notes",
            "lumina-subtitle",
            "tauri",
            "libmpv",
            "serde_json",
        ] {
            assert!(
                !manifest.contains(banned),
                "core must not depend on {banned}: {manifest}"
            );
        }
    }

    #[test]
    fn window_resolution_matches_mcp_semantics() {
        assert_eq!(
            resolve_window(None, None, Some(3), 60, 60),
            (3, 3),
            "radiusSec mirrors"
        );
        assert_eq!(
            resolve_window(Some(3), Some(2), None, 60, 60),
            (3, 2),
            "asymmetric window"
        );
        assert_eq!(
            resolve_window(None, None, None, 60, 60),
            (60, 60),
            "defaults"
        );
        assert_eq!(
            resolve_window(Some(999), Some(999), None, 60, 60),
            (300, 300),
            "each side caps at 300"
        );
        assert_eq!(
            resolve_window(None, None, Some(999), 60, 60),
            (300, 300),
            "radius caps at 300"
        );
    }

    #[test]
    fn capture_window_matches_mcp_semantics() {
        assert_eq!(resolve_capture_window(None, None, None), (0, 0));
        assert_eq!(resolve_capture_window(Some(2), Some(2), None), (2, 2));
        assert_eq!(
            resolve_capture_window(Some(99), Some(99), None),
            (7, 7),
            "capture caps at 7"
        );
    }

    #[test]
    fn center_resolution_matches_mcp_semantics() {
        assert_eq!(
            resolve_center_ms(None, None, 125_000, Some(3_600_000)),
            125_000
        );
        assert_eq!(resolve_center_ms(Some(90_000), None, 125_000, None), 90_000);
        assert_eq!(resolve_center_ms(None, Some(120), 125_000, None), 120_000);
        assert_eq!(
            resolve_center_ms(Some(60_000), Some(120), 125_000, None),
            60_000,
            "centerMs beats atSec"
        );
        assert_eq!(
            resolve_center_ms(Some(9_000_000), None, 125_000, Some(3_600_000)),
            3_600_000,
            "clamps to duration"
        );
    }

    #[test]
    fn cues_paging_matches_mcp_semantics() {
        assert_eq!(clamp_cues_offset(None), 0);
        assert_eq!(clamp_cues_limit(None), 80);
        assert_eq!(clamp_cues_limit(Some(2)), 2);
        assert_eq!(clamp_cues_limit(Some(999)), 200);
        assert_eq!(paginate(3, 0, 2), (0, 2, true));
        assert_eq!(paginate(3, 2, 2), (2, 1, false));
        assert_eq!(paginate(3, 99, 2), (3, 0, false));
        assert_eq!(paginate(0, 0, 80), (0, 0, false));
    }

    #[test]
    fn season_episode_bounds_match_schema() {
        assert_eq!(validate_season_episode(Some(2), Some(5)), Ok((2, 5)));
        assert_eq!(
            validate_season_episode(None, Some(1)),
            Err(SeasonEpisodeError::MissingSeason)
        );
        assert_eq!(
            validate_season_episode(Some(1), None),
            Err(SeasonEpisodeError::MissingEpisode)
        );
        assert_eq!(
            validate_season_episode(Some(0), Some(1)),
            Err(SeasonEpisodeError::NonPositive)
        );
    }

    #[test]
    fn write_batch_bounds_match_schema() {
        assert_eq!(validate_write_cues_len(0), Err(WriteCuesError::Empty));
        assert!(validate_write_cues_len(1).is_ok());
        assert!(validate_write_cues_len(WRITE_CUES_MAX).is_ok());
        assert_eq!(
            validate_write_cues_len(WRITE_CUES_MAX + 1),
            Err(WriteCuesError::TooMany)
        );
    }

    #[test]
    fn scene_helpers_match_mcp_semantics() {
        assert!(is_scene_mode(Some("scene")));
        assert!(is_scene_mode(Some("Scene")));
        assert!(!is_scene_mode(None));
        assert!(!is_scene_mode(Some("uniform")));
        assert_eq!(clamp_scene_threshold(None, 0.4), 0.4);
        assert_eq!(clamp_scene_threshold(Some(0.7), 0.4), 0.7);
        assert_eq!(clamp_scene_threshold(Some(5.0), 0.4), 0.9);
        assert_eq!(clamp_scene_threshold(Some(-1.0), 0.4), 0.1);
    }
}
