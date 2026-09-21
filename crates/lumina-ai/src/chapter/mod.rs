//! Pure chapter-domain evidence preparation.

mod evidence;

pub use evidence::{
    build_chapter_evidence, build_screenshot_reference, build_transcript_windows, ChapterEvidence,
    EvidenceBuildError, ScreenshotMetadata,
};
