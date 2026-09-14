//! Local media-library index and metadata resolver (library crate).
//!
//! Network resolution remains optional. Agent-backed resolution goes through
//! the app-injected core `AgentInvoker` port; model discovery orchestration
//! lives in the app adapter.

pub mod credentials;
mod error;
mod glossary;
mod metadata;
pub mod model;
mod naming;
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
    AgentModelDiscoveryConfig, AgentModelDiscoveryResult, AgentModelOption, AgentModelOptions,
    CredentialValidationConfig, CredentialValidationItem, CredentialValidationResult, EpisodeFile,
    EpisodeIndexEntry, GroupResolution, LibraryIndex, LibraryScanEvent, LibraryStatus,
    LibraryWatchConfig, MediaGroup, MediaGroupKind, MediaMetadataContext, MergedMediaContext,
    MetadataMediaType, MetadataWriteResult, ModelDiscoveryConfig, ModelDiscoveryResult,
    ModelResolverConfig, PendingMediaGroup, ResolverIntent, ResolverPreview,
    ResolverProviderConfig, ResolverRunConfig, ResolverSelection, SeriesLibraryCache,
    SeriesReading, StoredMetadata, StoredMetadataKind, TmdbCandidate, TmdbConfig,
    TmdbFieldSelection, TmdbGroupStatus, WikiCharacter, WikiEnrichmentCandidate,
    WikiEnrichmentPreview, WikiEpisodeSummary, WikiGroupStatus, WikiMatchMethod, WikiMetadata,
    WikiWriteResult, WikiZhReference, WIKI_STALE_AFTER_MS,
};
pub use naming::{parse_filename, ParsedName};
pub use paths::{discover_library_root_for_media, lumina_agent_context_path, lumina_tmp_dir};
pub use resolver::{
    discover_models, search_tmdb_direct, validate_credentials, validate_tmdb_credentials,
    RemoteResolver,
};
pub use service::MediaLibraryService;
pub use store::load as load_library_index;
