//! Local media-library index and future metadata resolver boundary.
//!
//! Network resolution remains optional. When users explicitly select an ACP
//! Agent provider, it is spawned only for an isolated, tool-disabled resolver
//! session; interactive chat sessions are never reused.

pub(crate) mod credentials;
mod error;
mod metadata;
pub mod model;
mod paths;
mod resolver;
mod scanner;
mod service;
mod store;
mod wikipedia;
mod wikitext;

pub use credentials::{CredentialKind, CredentialSaveInput, CredentialStatus};
pub use error::{LibraryError, LibraryErrorCode};
pub use metadata::{
    episode_index_for_group, load_context_at_root, load_context_for_group,
    resolve_episode_media_file, resolve_media_in_index, series_cache_from_context,
};
pub use model::{
    AgentModelDiscoveryConfig, AgentModelDiscoveryResult, CredentialValidationConfig,
    CredentialValidationItem, CredentialValidationResult, EpisodeIndexEntry, GroupResolution,
    LibraryIndex, LibraryScanEvent, LibraryStatus, LibraryWatchConfig, MediaGroup, MediaGroupKind,
    MediaMetadataContext, MergedMediaContext, MetadataMediaType, MetadataWriteResult,
    ModelDiscoveryConfig, ModelDiscoveryResult, ModelResolverConfig, PendingMediaGroup,
    ResolverIntent, ResolverPreview, ResolverProviderConfig, ResolverRunConfig, ResolverSelection,
    SeriesLibraryCache, StoredMetadata, StoredMetadataKind, TmdbCandidate, TmdbConfig,
    TmdbGroupStatus, WikiCharacter, WikiEnrichmentCandidate, WikiEnrichmentPreview,
    WikiEpisodeSummary, WikiGroupStatus, WikiMatchMethod, WikiMetadata, WikiWriteResult,
    WikiZhReference, WIKI_STALE_AFTER_MS,
};
pub use paths::discover_library_root_for_media;
pub(crate) use paths::{lumina_agent_context_path, lumina_tmp_dir};
pub use resolver::{
    discover_agent_models, discover_models, validate_credentials, validate_tmdb_credentials,
    RemoteResolver,
};
pub use service::MediaLibraryService;
pub use store::load as load_library_index;
