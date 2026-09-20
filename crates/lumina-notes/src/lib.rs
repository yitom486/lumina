//! Local timestamped notes + Markdown export (library crate).

use std::path::Path;

use lumina_subtitle::{SubtitleError, SubtitleService, Transcript};

pub mod error;
pub mod headings;
pub mod model;
pub mod proposal;
pub mod quotes;
pub mod service;
pub mod store;

pub use error::{NoteError, NoteErrorCode};
pub use headings::{resolve_export_headings, NotesExportHeadings, NotesMediaMetadata};
pub use model::{
    Note, NoteCreate, NoteFrame, NoteFrameData, NotePreviewQuotes, NoteQuote, NoteUpdate,
};
pub use proposal::VideoAnnotationProposal;
pub use service::NoteService;

pub(crate) fn load_subtitle_choice(
    media_path: &Path,
    choice_id: &str,
) -> Result<Transcript, SubtitleError> {
    if lumina_ytdl::provider::parse_cache_choice(choice_id).is_some() {
        lumina_ytdl::provider::load_cached_choice(&media_path.to_string_lossy(), choice_id)
    } else {
        SubtitleService::load_choice(media_path, choice_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_choices_use_ytdl_cache_loader() {
        let media_path = format!(
            "D:\\video\\lumina-notes-missing-cache-{}.mp4",
            std::process::id()
        );
        let error = load_subtitle_choice(Path::new(&media_path), "cache:subdl:zh")
            .expect_err("missing cached subtitle should fail through the cache loader");

        assert_eq!(error.message, "无法提取字幕");
        assert_eq!(
            error.details.as_deref(),
            Some("cached subtitle unavailable")
        );
    }
}
