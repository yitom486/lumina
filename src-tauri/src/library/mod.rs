//! Local media-library index and future metadata resolver boundary.
//!
//! This module intentionally does not spawn ACP/Codex or make network calls.
//! A future configured resolver will consume `ResolverIntent`/`TmdbCandidate`
//! and write confirmed TMDb metadata into the existing `.lumina` group folder.

mod error;
mod metadata;
pub mod model;
mod resolver;
mod scanner;
mod service;
mod store;

pub use error::{LibraryError, LibraryErrorCode};
pub use model::{
    GroupResolution, LibraryIndex, LibraryStatus, LibraryWatchConfig, MediaGroup, MediaGroupKind,
    MetadataMediaType, MetadataWriteResult, ModelResolverConfig, PendingMediaGroup, ResolverIntent,
    ResolverPreview, ResolverRunConfig, ResolverSelection, StoredMetadata, StoredMetadataKind,
    TmdbCandidate, TmdbConfig,
};
pub use resolver::RemoteResolver;
pub use service::MediaLibraryService;
