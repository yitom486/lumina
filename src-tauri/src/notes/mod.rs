//! Local timestamped notes + Markdown export.

pub mod error;
pub mod model;
pub mod service;
pub mod store;

pub use error::{NoteError, NoteErrorCode};
pub use model::{Note, NoteCreate, NoteUpdate};
pub use service::NoteService;
