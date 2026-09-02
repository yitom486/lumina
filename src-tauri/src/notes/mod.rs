//! Local timestamped notes + Markdown export.

pub mod error;
pub mod headings;
pub mod model;
pub mod quotes;
pub mod service;
pub mod store;

pub use error::{NoteError, NoteErrorCode};
pub use headings::{NotesExportHeadings, resolve_export_headings};
pub use model::{Note, NoteCreate, NotePreviewQuotes, NoteQuote, NoteUpdate};
pub use service::NoteService;
