//! Pure builders for evidence supplied to the chapter agent.
//!
//! A transcript window is only an evidence container. It is not a chapter and
//! must never be used as a replacement for semantic segmentation.

use std::collections::BTreeMap;
use std::fmt;

use lumina_subtitle::Cue;

use crate::prompts::{ScreenshotReference, TranscriptLine, TranscriptWindow};

/// Metadata produced by the host after capturing one screenshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotMetadata {
    pub asset_id: String,
    pub timestamp_ms: u64,
    pub resource_ref: String,
    pub note: Option<String>,
    pub media_duration_ms: Option<u64>,
}

/// Errors for invalid evidence metadata supplied by the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceBuildError {
    ZeroWindowWidth,
    EmptyScreenshotAssetId,
    EmptyScreenshotResource,
    ScreenshotOutsideMedia,
}

impl fmt::Display for EvidenceBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ZeroWindowWidth => "evidence window width must be greater than zero",
            Self::EmptyScreenshotAssetId => "screenshot asset id must not be empty",
            Self::EmptyScreenshotResource => "screenshot resource reference must not be empty",
            Self::ScreenshotOutsideMedia => "screenshot timestamp must be inside the media",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for EvidenceBuildError {}

/// Build stable, ordered transcript evidence windows from subtitle cues.
///
/// Cues are assigned by their start timestamp to a fixed-width bucket. Empty
/// text and non-positive cue intervals are ignored. A cue that crosses a
/// bucket boundary remains intact in the bucket where it starts, so the Agent
/// receives the original dialogue timing instead of a fabricated split line.
pub fn build_transcript_windows(
    cues: &[Cue],
    window_width_ms: u64,
) -> Result<Vec<TranscriptWindow>, EvidenceBuildError> {
    if window_width_ms == 0 {
        return Err(EvidenceBuildError::ZeroWindowWidth);
    }

    let mut buckets: BTreeMap<u64, Vec<&Cue>> = BTreeMap::new();
    for cue in cues {
        if cue.start_ms >= cue.end_ms || cue.text.trim().is_empty() {
            continue;
        }
        let bucket = (cue.start_ms / window_width_ms) * window_width_ms;
        buckets.entry(bucket).or_default().push(cue);
    }

    let mut windows = Vec::with_capacity(buckets.len());
    for (ordinal, (_, mut bucket_cues)) in buckets.into_iter().enumerate() {
        bucket_cues.sort_by_key(|cue| (cue.start_ms, cue.end_ms, cue.index));
        let lines: Vec<TranscriptLine> = bucket_cues
            .into_iter()
            .map(|cue| TranscriptLine {
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                text: cue.text.trim().to_string(),
            })
            .collect();

        let Some(start_ms) = lines.first().map(|line| line.start_ms) else {
            continue;
        };
        let end_ms = lines
            .iter()
            .map(|line| line.end_ms)
            .max()
            .unwrap_or(start_ms);
        windows.push(TranscriptWindow {
            window_id: format!("transcript-window-{ordinal:04}"),
            start_ms,
            end_ms,
            lines,
        });
    }

    Ok(windows)
}

/// Convert host-owned screenshot metadata into a validator-visible reference.
pub fn build_screenshot_reference(
    metadata: ScreenshotMetadata,
) -> Result<ScreenshotReference, EvidenceBuildError> {
    let asset_id = metadata.asset_id.trim();
    if asset_id.is_empty() {
        return Err(EvidenceBuildError::EmptyScreenshotAssetId);
    }
    let resource_ref = metadata.resource_ref.trim();
    if resource_ref.is_empty() {
        return Err(EvidenceBuildError::EmptyScreenshotResource);
    }
    if metadata
        .media_duration_ms
        .is_some_and(|duration_ms| metadata.timestamp_ms > duration_ms)
    {
        return Err(EvidenceBuildError::ScreenshotOutsideMedia);
    }

    Ok(ScreenshotReference {
        asset_id: asset_id.to_string(),
        timestamp_ms: metadata.timestamp_ms,
        resource_ref: resource_ref.to_string(),
        note: metadata
            .note
            .and_then(|note| (!note.trim().is_empty()).then(|| note.trim().to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(index: u32, start_ms: u64, end_ms: u64, text: &str) -> Cue {
        Cue {
            index,
            start_ms,
            end_ms,
            text: text.to_string(),
        }
    }

    #[test]
    fn transcript_windows_are_stable_and_keep_cross_boundary_cues_intact() {
        let cues = vec![
            cue(4, 10_100, 11_500, " second "),
            cue(1, 1_000, 2_000, " first "),
            cue(2, 9_500, 10_500, " crosses "),
            cue(3, 0, 1, "   "),
            cue(5, 20, 20, "invalid"),
        ];

        let first = build_transcript_windows(&cues, 10_000).expect("valid width");
        let second = build_transcript_windows(&cues, 10_000).expect("valid width");

        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].window_id, "transcript-window-0000");
        assert_eq!(first[0].start_ms, 1_000);
        assert_eq!(first[0].end_ms, 10_500);
        assert_eq!(first[0].lines[0].text, "first");
        assert_eq!(first[1].start_ms, 10_100);
        assert_eq!(first[1].end_ms, 11_500);
        assert_eq!(first[0].lines.len(), 2);
        assert_eq!(first[1].lines.len(), 1);
    }

    #[test]
    fn empty_or_invalid_cues_are_not_evidence() {
        let cues = vec![cue(1, 5, 5, "same"), cue(2, 6, 7, "  ")];
        let windows = build_transcript_windows(&cues, 1_000).expect("valid width");
        assert!(windows.is_empty());
    }

    #[test]
    fn zero_window_width_is_rejected() {
        let error = build_transcript_windows(&[], 0).expect_err("zero width must fail");
        assert_eq!(error, EvidenceBuildError::ZeroWindowWidth);
    }

    #[test]
    fn screenshot_reference_trims_text_and_validates_bounds() {
        let reference = build_screenshot_reference(ScreenshotMetadata {
            asset_id: " asset-1 ".to_string(),
            timestamp_ms: 900,
            resource_ref: " frame.jpg ".to_string(),
            note: Some(" note ".to_string()),
            media_duration_ms: Some(1_000),
        })
        .expect("valid screenshot");
        assert_eq!(reference.asset_id, "asset-1");
        assert_eq!(reference.resource_ref, "frame.jpg");
        assert_eq!(reference.note.as_deref(), Some("note"));

        let error = build_screenshot_reference(ScreenshotMetadata {
            asset_id: "asset-2".to_string(),
            timestamp_ms: 1_001,
            resource_ref: "frame.jpg".to_string(),
            note: None,
            media_duration_ms: Some(1_000),
        })
        .expect_err("outside media must fail");
        assert_eq!(error, EvidenceBuildError::ScreenshotOutsideMedia);
    }

    #[test]
    fn screenshot_reference_rejects_empty_identity_and_resource() {
        let base = ScreenshotMetadata {
            asset_id: " ".to_string(),
            timestamp_ms: 0,
            resource_ref: "frame.jpg".to_string(),
            note: None,
            media_duration_ms: None,
        };
        assert_eq!(
            build_screenshot_reference(base).expect_err("empty id must fail"),
            EvidenceBuildError::EmptyScreenshotAssetId
        );

        let error = build_screenshot_reference(ScreenshotMetadata {
            asset_id: "asset".to_string(),
            timestamp_ms: 0,
            resource_ref: " ".to_string(),
            note: None,
            media_duration_ms: None,
        })
        .expect_err("empty resource must fail");
        assert_eq!(error, EvidenceBuildError::EmptyScreenshotResource);
    }
}
