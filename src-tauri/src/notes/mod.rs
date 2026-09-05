//! Local timestamped notes + Markdown export.

pub mod error;
pub mod headings;
pub mod model;
pub mod proposal;
pub mod quotes;
pub mod service;
pub mod store;

pub use error::{NoteError, NoteErrorCode};
pub use headings::{resolve_export_headings, NotesExportHeadings};
pub use model::{
    Note, NoteCreate, NoteFrame, NoteFrameData, NotePreviewQuotes, NoteQuote, NoteUpdate,
};
pub use proposal::VideoAnnotationProposal;
pub use service::NoteService;
