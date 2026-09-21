#![allow(dead_code)]

//! Minimal synchronous chapter evidence worker.
//!
//! The worker is deliberately independent from the player and the main ACP
//! service.  It currently prepares and validates local evidence, which gives
//! the later DB/Chapter-session batch a real execution seam without inventing
//! APIs that are not present in this checkout.

use std::path::{Path, PathBuf};

use lumina_acp::AgentProfilesHint;
use lumina_ai::chapter::{
    build_screenshot_reference, build_transcript_windows, ScreenshotMetadata,
};
use lumina_media::frame_capture::{
    capture_frames, detect_scene_times, select_keyframes, DEFAULT_SCENE_THRESHOLD,
};
use lumina_media::MediaInspector;
use lumina_subtitle::SubtitleService;
use serde::de::DeserializeOwned;

use super::chapter::{ChapterError, ChapterSegmentationRequest};

pub const TRANSCRIPT_WINDOW_WIDTH_MS: u64 = 30_000;
pub const DEFAULT_CAPTURE_BUDGET: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonFenceError {
    Empty,
    InvalidFence,
    InvalidJson,
}

/// Parse either a JSON document or a single ```json fenced JSON document.
/// This helper deliberately does not accept prose around the document.
pub fn parse_json_fence<T: DeserializeOwned>(text: &str) -> Result<T, JsonFenceError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(JsonFenceError::Empty);
    }
    let payload = if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        let Some(opening) = lines.next() else {
            return Err(JsonFenceError::InvalidFence);
        };
        let language = opening.trim().trim_start_matches("```").trim();
        if !language.is_empty() && !language.eq_ignore_ascii_case("json") {
            return Err(JsonFenceError::InvalidFence);
        }
        let Some(closing) = lines.next_back() else {
            return Err(JsonFenceError::InvalidFence);
        };
        if closing.trim() != "```" {
            return Err(JsonFenceError::InvalidFence);
        }
        lines.collect::<Vec<_>>().join("\n")
    } else {
        trimmed.to_owned()
    };
    serde_json::from_str(&payload).map_err(|_| JsonFenceError::InvalidJson)
}

pub fn has_complete_worker_config(
    profile_id: Option<&str>,
    profiles: Option<&AgentProfilesHint>,
) -> bool {
    let Some(profile_id) = profile_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let Some(profiles) = profiles else {
        return false;
    };
    profiles
        .profiles
        .iter()
        .any(|profile| profile.id == profile_id && !profile.command.trim().is_empty())
}

/// Run the local, non-UI part of a chapter task.
pub fn run(request: ChapterSegmentationRequest) -> Result<(), ChapterError> {
    validate_request(&request)?;
    let media_path = PathBuf::from(&request.media_path);
    if !media_path.is_file() {
        return Err(ChapterError::media_missing(Some(
            "media file is not readable",
        )));
    }

    let media = MediaInspector::inspect(&media_path).map_err(|error| {
        tracing::warn!(%error, "chapter media inspection failed");
        ChapterError::probe_failed(Some(&error.to_string()))
    })?;

    let transcript = load_transcript(&media_path, request.subtitle_choice_id.as_deref())?;
    let transcript_windows = build_transcript_windows(
        transcript.as_deref().unwrap_or(&[]),
        TRANSCRIPT_WINDOW_WIDTH_MS,
    )
    .map_err(|error| ChapterError::subtitle_failed(Some(&error.to_string())))?;

    let screenshots = capture_evidence(
        &media_path,
        &request.task_key(),
        media.duration_ms,
        request.position_ms.unwrap_or(0),
    )?;

    tracing::info!(
        task_key = %request.task_key(),
        transcript_windows = transcript_windows.len(),
        screenshots = screenshots.len(),
        "chapter evidence prepared"
    );
    Ok(())
}

fn validate_request(request: &ChapterSegmentationRequest) -> Result<(), ChapterError> {
    if request.media_path.trim().is_empty() || request.episode_key.trim().is_empty() {
        return Err(ChapterError::invalid_input(Some("empty task identity")));
    }
    if is_remote_url(&request.media_path) {
        return Err(ChapterError::remote_media(Some("remote media path")));
    }
    if !has_complete_worker_config(request.profile_id.as_deref(), request.profiles.as_ref()) {
        return Err(ChapterError::invalid_input(Some(
            "profileId and profiles must describe an available profile",
        )));
    }
    Ok(())
}

fn is_remote_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn load_transcript(
    media_path: &Path,
    choice_id: Option<&str>,
) -> Result<Option<Vec<lumina_subtitle::Cue>>, ChapterError> {
    let choices = SubtitleService::list_choices(media_path).map_err(|error| {
        tracing::warn!(%error, "chapter subtitle listing failed");
        ChapterError::subtitle_failed(Some(&error.to_string()))
    })?;
    let selected = match choice_id {
        Some(choice_id) => choices
            .iter()
            .find(|choice| choice.id == choice_id && choice.supported),
        None => choices.iter().find(|choice| choice.supported),
    };
    let Some(selected) = selected else {
        // A chapter task can proceed with visual evidence alone.
        return Ok(None);
    };
    let transcript = SubtitleService::load_choice(media_path, &selected.id).map_err(|error| {
        tracing::warn!(%error, "chapter subtitle loading failed");
        ChapterError::subtitle_failed(Some(&error.to_string()))
    })?;
    Ok(Some(transcript.cues))
}

fn capture_evidence(
    media_path: &Path,
    task_key: &str,
    duration_ms: Option<u64>,
    position_ms: u64,
) -> Result<Vec<lumina_ai::prompts::ScreenshotReference>, ChapterError> {
    let coverage = build_coverage_times(duration_ms, position_ms, DEFAULT_CAPTURE_BUDGET);
    let coverage_seconds: Vec<f64> = coverage
        .iter()
        .map(|value| *value as f64 / 1_000.0)
        .collect();
    let scenes = detect_scene_times(media_path, DEFAULT_SCENE_THRESHOLD).map_err(|error| {
        tracing::warn!(%error, "chapter scene detection failed");
        ChapterError::capture_failed(Some(&error.to_string()))
    })?;
    let selected = select_keyframes(&coverage_seconds, &scenes, DEFAULT_CAPTURE_BUDGET);

    let parent = media_path
        .parent()
        .ok_or_else(|| ChapterError::capture_failed(Some("media has no parent directory")))?;
    let capture_dir = parent
        .join(".lumina")
        .join("tmp")
        .join(format!("chapter-capture-{}", stable_task_id(task_key)));
    let files = capture_frames(media_path, &selected, &capture_dir).map_err(|error| {
        tracing::warn!(%error, "chapter frame capture failed");
        ChapterError::capture_failed(Some(&error.to_string()))
    })?;

    files
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let timestamp_ms = selected
                .get(index)
                .and_then(|value| seconds_to_millis(*value))
                .ok_or_else(|| ChapterError::capture_failed(Some("invalid capture timestamp")))?;
            build_screenshot_reference(&ScreenshotMetadata {
                asset_id: format!("chapter-frame-{index:02}"),
                timestamp_ms: Some(timestamp_ms),
                resource_ref: path.to_string_lossy().to_string(),
                note: None,
            })
            .map_err(|error| ChapterError::capture_failed(Some(&error.to_string())))
        })
        .collect()
}

/// Build a bounded, deterministic coverage grid and keep the requested
/// position as an anchor. This helper is pure and does not touch ffmpeg.
pub fn build_coverage_times(duration_ms: Option<u64>, position_ms: u64, budget: usize) -> Vec<u64> {
    if budget == 0 {
        return Vec::new();
    }
    let end_ms = duration_ms.unwrap_or(position_ms).max(position_ms);
    if budget == 1 || end_ms == 0 {
        return vec![position_ms.min(end_ms)];
    }
    let last = budget - 1;
    let mut times: Vec<u64> = (0..budget)
        .map(|index| end_ms.saturating_mul(index as u64) / last as u64)
        .collect();
    let anchor = position_ms.min(end_ms);
    if !times.contains(&anchor) {
        times.push(anchor);
        times.sort_unstable();
        times.dedup();
        if times.len() > budget {
            times = select_uniform_budget(&times, budget);
        }
    }
    times
}

fn select_uniform_budget(values: &[u64], budget: usize) -> Vec<u64> {
    if values.len() <= budget {
        return values.to_vec();
    }
    if budget == 1 {
        return vec![values[values.len() / 2]];
    }
    let last = values.len() - 1;
    (0..budget)
        .map(|index| values[index * last / (budget - 1)])
        .collect()
}

fn seconds_to_millis(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let millis = value * 1_000.0;
    if millis > u64::MAX as f64 {
        return None;
    }
    Some(millis.round() as u64)
}

fn stable_task_id(value: &str) -> String {
    let hash = value.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
        hash ^ u64::from(byte).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_keeps_anchor_and_respects_budget() {
        let values = build_coverage_times(Some(120_000), 30_000, 5);
        assert!(values.contains(&30_000));
        assert!(values.len() <= 5);
        assert!(values.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn coverage_zero_budget_is_empty() {
        assert!(build_coverage_times(Some(120_000), 30_000, 0).is_empty());
    }

    #[test]
    fn json_parser_accepts_plain_and_json_fenced_documents() {
        let plain: serde_json::Value = match parse_json_fence(r#"{"chapters":[]}"#) {
            Ok(value) => value,
            Err(error) => panic!("plain JSON should parse: {error:?}"),
        };
        let fenced: serde_json::Value = match parse_json_fence("```json\n{\"chapters\": []}\n```") {
            Ok(value) => value,
            Err(error) => panic!("JSON fence should parse: {error:?}"),
        };
        assert_eq!(plain, fenced);
    }

    #[test]
    fn json_parser_rejects_prose_and_non_json_fences() {
        assert_eq!(
            parse_json_fence::<serde_json::Value>("```rust\n{}\n```")
                .expect_err("rust fence must be rejected"),
            JsonFenceError::InvalidFence
        );
        assert_eq!(
            parse_json_fence::<serde_json::Value>("not json").expect_err("prose must be rejected"),
            JsonFenceError::InvalidJson
        );
    }

    #[test]
    fn remote_urls_are_rejected_without_io() {
        assert!(is_remote_url("https://example.test/video"));
        assert!(is_remote_url("HTTP://example.test/video"));
        assert!(!is_remote_url("C:/video/a.mkv"));
    }
}
