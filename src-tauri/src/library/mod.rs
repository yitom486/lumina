//! Local media-library index and future metadata resolver boundary.
//!
//! Network resolution remains optional. When users explicitly select an ACP
//! Agent provider, it is spawned only for an isolated, tool-disabled resolver
//! session; interactive chat sessions are never reused.

pub(crate) mod credentials;
mod error;
mod metadata;
pub mod model;
mod resolver;
mod scanner;
mod service;
mod store;
mod wikipedia;
mod wikitext;

pub use credentials::{CredentialKind, CredentialSaveInput, CredentialStatus};
pub use error::{LibraryError, LibraryErrorCode};
pub use model::{
    AgentModelDiscoveryConfig, AgentModelDiscoveryResult, CredentialValidationConfig,
    CredentialValidationItem, CredentialValidationResult, GroupResolution, LibraryIndex,
    LibraryScanEvent, LibraryStatus, LibraryWatchConfig, MediaGroup, MediaGroupKind,
    MediaMetadataContext,
    MergedMediaContext, MetadataMediaType, MetadataWriteResult, ModelDiscoveryConfig,
    ModelDiscoveryResult, ModelResolverConfig, PendingMediaGroup, ResolverIntent, ResolverPreview,
    ResolverProviderConfig, ResolverRunConfig, ResolverSelection, StoredMetadata,
    StoredMetadataKind, TmdbCandidate, TmdbConfig, WikiEnrichmentCandidate, WikiEnrichmentPreview,
    WikiMatchMethod, WikiMetadata, WikiCharacter, WikiEpisodeSummary, WikiWriteResult,
    WikiZhReference, WikiGroupStatus, WIKI_STALE_AFTER_MS, TmdbGroupStatus,
};
pub use resolver::{
    discover_agent_models, discover_models, validate_credentials, validate_tmdb_credentials,
    RemoteResolver,
};
pub use service::MediaLibraryService;
