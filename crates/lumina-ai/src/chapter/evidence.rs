//! Build deterministic evidence blocks for chapter workers.
//!
//! This module deliberately contains no file I/O, media processing, process
//! lifecycle, or agent protocol code. Fixed-width windows are only evidence
//! partitions; they must not be interpreted as chapter segmentation.

use std::fmt;
use std::collections::BTreeMap;

use lumina_subtitle::Cue;

use crate::prompts::{ScreenshotReference, TranscriptWindow};

const WINDOW_ID_PREFIX: &str = "transcript-window";

/// Metadata emitted by a screenshot-producing layer before prompt assembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotMetadata {
    pub asset_id: String,
    pub timestamp_ms: Option<u64>,
    pub resource_ref: String,
    pub note: Option<String>,
}

/// The pure evidence payload consumed together by a chapter worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterEvidence {
    pub transcript_windows: Vec<TranscriptWindow>,
    pub screenshots: Vec<ScreenshotReference>,
}

/// Domain validation failures while preparing chapter evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceBuildError {
    InvalidWindowWidth,
    EmptyScreenshotAssetId,
    MissingScreenshotTimestamp,
    InvalidScreenshotTimestamp,
    EmptyScreenshotResourceRef,
}

impl fmt::Display for EvidenceBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidWindowWidth => "字幕证据窗口宽度必须大于零",
            Self::EmptyScreenshotAssetId => "截图证据缺少资源标识",
            Self::MissingScreenshotTimestamp => "截图证据缺少时间点",
            Self::InvalidScreenshotTimestamp => "截图证据时间点无效",
            Self::EmptyScreenshotResourceRef => "截图证据缺少资源引用",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for EvidenceBuildError {}

/// Build fixed-width transcript evidence windows.
///
/// Cues with empty text or an invalid half-open interval (`start_ms >=
/// end_ms`) are ignored. Valid cues are ordered by timeline and included in
/// every bucket they overlap, so a cue crossing a boundary is available from
/// both sides without being split. Empty buckets are omitted. The returned
/// window bounds are clipped to the actual ranges of the cues in that bucket.
pub fn build_transcript_windows(
    cues: &[Cue],
    window_width_ms: u64,
) -> Result<Vec<TranscriptWindow>, EvidenceBuildError> {
    if window_width_ms == 0 {
        return Err(EvidenceBuildError::InvalidWindowWidth);
    }

    let mut valid_cues: Vec<Cue> = cues
        .iter()
        .filter(|cue| is_valid_cue(cue))
        .cloned()
        .collect();
    valid_cues.sort_by_key(|cue| (cue.start_ms, cue.end_ms, cue.index));

    let mut windows_by_bucket: BTreeMap<u64, TranscriptWindow> = BTreeMap::new();
    for cue in valid_cues {
        let first_bucket = cue.start_ms / window_width_ms;
        let last_bucket = (cue.end_ms - 1) / window_width_ms;

        for bucket in first_bucket..=last_bucket {
            let bucket_start = bucket.saturating_mul(window_width_ms);
            let bucket_end = bucket_start
                .checked_add(window_width_ms)
                .unwrap_or(u64::MAX);
            let clipped_start = cue.start_ms.max(bucket_start);
            let clipped_end = cue.end_ms.min(bucket_end);

            if clipped_start >= clipped_end {
                continue;
            }

            if let Some(window) = windows_by_bucket.get_mut(&bucket) {
                window.start_ms = window.start_ms.min(clipped_start);
                window.end_ms = window.end_ms.max(clipped_end);
                window.cues.push(cue.clone());
            } else {
                windows_by_bucket.insert(bucket, TranscriptWindow {
                    window_id: transcript_window_id(bucket),
                    start_ms: clipped_start,
                    end_ms: clipped_end,
                    cues: vec![cue.clone()],
                });
            }
        }
    }

    Ok(windows_by_bucket.into_values().collect())
}

/// Construct a validated screenshot reference from generated-asset metadata.
pub fn build_screenshot_reference(
    metadata: &ScreenshotMetadata,
) -> Result<ScreenshotReference, EvidenceBuildError> {
    let asset_id = metadata.asset_id.trim();
    if asset_id.is_empty() {
        return Err(EvidenceBuildError::EmptyScreenshotAssetId);
    }

    let timestamp_ms = metadata
        .timestamp_ms
        .ok_or(EvidenceBuildError::MissingScreenshotTimestamp)?;
    if timestamp_ms == u64::MAX {
        return Err(EvidenceBuildError::InvalidScreenshotTimestamp);
    }

    let resource_ref = metadata.resource_ref.trim();
    if resource_ref.is_empty() {
        return Err(EvidenceBuildError::EmptyScreenshotResourceRef);
    }

    let note = metadata
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty())
        .map(str::to_owned);

    Ok(ScreenshotReference {
        asset_id: asset_id.to_owned(),
        timestamp_ms,
        resource_ref: resource_ref.to_owned(),
        note,
    })
}

/// Build the complete transcript-and-screenshot payload for one worker call.
///
/// Screenshot validation is all-or-nothing: the first invalid metadata item
/// is returned and no partial payload is produced.
pub fn build_chapter_evidence(
    cues: &[Cue],
    window_width_ms: u64,
    screenshots: &[ScreenshotMetadata],
) -> Result<ChapterEvidence, EvidenceBuildError> {
    let transcript_windows = build_transcript_windows(cues, window_width_ms)?;
    let screenshots = screenshots
        .iter()
        .map(build_screenshot_reference)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ChapterEvidence {
        transcript_windows,
        screenshots,
    })
}

fn is_valid_cue(cue: &Cue) -> bool {
    cue.start_ms < cue.end_ms && !cue.text.trim().is_empty()
}

fn transcript_window_id(bucket: u64) -> String {
    format!("{WINDOW_ID_PREFIX}-{bucket:016}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(index: u32, start_ms: u64, end_ms: u64, text: &str) -> Cue {
        Cue {
            index,
            start_ms,
            end_ms,
            text: text.to_owned(),
        }
    }

    #[test]
    fn windows_are_clipped_to_actual_cue_ranges() {
        let cues = [cue(1, 1_250, 2_250, "first"), cue(2, 3_100, 3_400, "second")];

        let windows = build_transcript_windows(&cues, 1_000);

        let windows = match windows {
            Ok(windows) => windows,
            Err(error) => panic!("unexpected error: {error}"),
        };
        assert_eq!(windows.len(), 3);
        assert_eq!((windows[0].start_ms, windows[0].end_ms), (1_250, 2_000));
        assert_eq!((windows[1].start_ms, windows[1].end_ms), (2_000, 2_250));
        assert_eq!((windows[2].start_ms, windows[2].end_ms), (3_100, 3_400));
    }

    #[test]
    fn cue_crossing_boundary_is_available_in_both_windows() {
        let cues = [cue(7, 900, 1_100, "crossing")];

        let windows = match build_transcript_windows(&cues, 1_000) {
            Ok(windows) => windows,
            Err(error) => panic!("unexpected error: {error}"),
        };

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].cues, cues);
        assert_eq!(windows[1].cues, cues);
        assert_eq!(windows[0].window_id, "transcript-window-0000000000000000");
        assert_eq!(windows[1].window_id, "transcript-window-0000000000000001");
    }

    #[test]
    fn empty_and_invalid_cues_are_skipped() {
        let cues = [
            cue(1, 0, 0, "zero length"),
            cue(2, 20, 10, "backwards"),
            cue(3, 10, 20, "   \n"),
            cue(4, 30, 40, "valid"),
        ];

        let windows = match build_transcript_windows(&cues, 100) {
            Ok(windows) => windows,
            Err(error) => panic!("unexpected error: {error}"),
        };

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].cues, vec![cues[3].clone()]);
        assert_eq!((windows[0].start_ms, windows[0].end_ms), (30, 40));
    }

    #[test]
    fn window_ids_are_stable_and_ordered() {
        let cues = [cue(2, 2_100, 2_200, "later"), cue(1, 100, 200, "earlier")];

        let first = match build_transcript_windows(&cues, 1_000) {
            Ok(windows) => windows,
            Err(error) => panic!("unexpected error: {error}"),
        };
        let second = match build_transcript_windows(&cues, 1_000) {
            Ok(windows) => windows,
            Err(error) => panic!("unexpected error: {error}"),
        };

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|window| window.window_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "transcript-window-0000000000000000",
                "transcript-window-0000000000000002",
            ]
        );
    }

    #[test]
    fn invalid_window_width_is_reported() {
        assert_eq!(
            build_transcript_windows(&[], 0),
            Err(EvidenceBuildError::InvalidWindowWidth)
        );
    }

    #[test]
    fn screenshot_metadata_is_validated_and_trimmed() {
        let metadata = ScreenshotMetadata {
            asset_id: " asset-1 ".to_owned(),
            timestamp_ms: Some(1_234),
            resource_ref: " resource://frame-1 ".to_owned(),
            note: Some("  establishing shot  ".to_owned()),
        };

        let reference = match build_screenshot_reference(&metadata) {
            Ok(reference) => reference,
            Err(error) => panic!("unexpected error: {error}"),
        };
        assert_eq!(reference.asset_id, "asset-1");
        assert_eq!(reference.timestamp_ms, 1_234);
        assert_eq!(reference.resource_ref, "resource://frame-1");
        assert_eq!(reference.note.as_deref(), Some("establishing shot"));
    }

    #[test]
    fn invalid_screenshot_metadata_never_panics() {
        let cases = [
            (
                ScreenshotMetadata {
                    asset_id: "  ".to_owned(),
                    timestamp_ms: Some(1),
                    resource_ref: "ref".to_owned(),
                    note: None,
                },
                EvidenceBuildError::EmptyScreenshotAssetId,
            ),
            (
                ScreenshotMetadata {
                    asset_id: "asset".to_owned(),
                    timestamp_ms: None,
                    resource_ref: "ref".to_owned(),
                    note: None,
                },
                EvidenceBuildError::MissingScreenshotTimestamp,
            ),
            (
                ScreenshotMetadata {
                    asset_id: "asset".to_owned(),
                    timestamp_ms: Some(u64::MAX),
                    resource_ref: "ref".to_owned(),
                    note: None,
                },
                EvidenceBuildError::InvalidScreenshotTimestamp,
            ),
            (
                ScreenshotMetadata {
                    asset_id: "asset".to_owned(),
                    timestamp_ms: Some(1),
                    resource_ref: "  ".to_owned(),
                    note: None,
                },
                EvidenceBuildError::EmptyScreenshotResourceRef,
            ),
        ];

        for (metadata, expected) in cases {
            assert_eq!(build_screenshot_reference(&metadata), Err(expected));
        }
    }

    #[test]
    fn chapter_evidence_returns_both_kinds_of_evidence_together() {
        let screenshots = [ScreenshotMetadata {
            asset_id: "asset-1".to_owned(),
            timestamp_ms: Some(500),
            resource_ref: "resource://asset-1".to_owned(),
            note: None,
        }];

        let evidence = match build_chapter_evidence(
            &[cue(1, 100, 900, "line")],
            1_000,
            &screenshots,
        ) {
            Ok(evidence) => evidence,
            Err(error) => panic!("unexpected error: {error}"),
        };

        assert_eq!(evidence.transcript_windows.len(), 1);
        assert_eq!(evidence.screenshots.len(), 1);
    }
}
