//! Local media-library index and future metadata resolver boundary.
//!
//! This module intentionally does not spawn ACP/Codex or make network calls.
//! A future configured resolver will consume `ResolverIntent`/`TmdbCandidate`
//! and write confirmed TMDb metadata into the existing `.lumina` group folder.

mod error;
pub mod model;
mod scanner;
mod service;
mod store;

pub use error::{LibraryError, LibraryErrorCode};
pub use model::{
    GroupResolution, LibraryIndex, LibraryStatus, LibraryWatchConfig, MediaGroup, MediaGroupKind,
    MetadataMediaType, PendingMediaGroup, ResolverIntent, ResolverSelection, TmdbCandidate,
};
pub use service::MediaLibraryService;
