//! Serializable local-library contracts. These are also the stable boundary for
//! future resolver tools; no model provider or TMDb transport is embedded here.

use serde::{Deserialize, Serialize};

use crate::acp::{AcpModelDiscoveryResult, AgentProfilesHint};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryWatchConfig {
    pub roots: Vec<String>,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
}

impl Default for LibraryWatchConfig {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            poll_interval_secs: default_poll_interval_secs(),
        }
    }
}

fn default_poll_interval_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStatus {
    pub running: bool,
    pub roots: Vec<String>,
    pub poll_interval_secs: u64,
    /// Timestamp of the most recent successful scan. Failed scans leave this
    /// intact so callers can tell stale data from fresh data.
    pub last_scan_at_ms: Option<u128>,
    /// Safe summary of the latest failed scan. Deliberately excludes paths and
    /// implementation details that are only useful in application logs.
    pub last_scan_error: Option<LibraryScanIssue>,
    pub indexed_files: usize,
    pub pending_groups: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryScanIssue {
    pub code: crate::library::error::LibraryErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "PascalCase")]
pub enum LibraryScanEvent {
    Started { root_count: usize },
    Progress { roots_completed: usize, root_count: usize, indexed_files: usize },
    Finished { indexed_files: usize, pending_groups: usize },
    Failed { code: crate::library::error::LibraryErrorCode, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryIndex {
    pub schema_version: u32,
    pub root: String,
    pub updated_at_ms: u128,
    pub files: Vec<IndexedMediaFile>,
    pub groups: Vec<MediaGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexedMediaFile {
    pub relative_path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_at_ms: u128,
    pub group_key: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaGroup {
    pub key: String,
    pub display_name: String,
    pub kind: MediaGroupKind,
    pub files: Vec<String>,
    /// User-provided fallback for an ambiguous release-name group. This is
    /// intentional local state, and must survive subsequent directory scans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_title: Option<String>,
    pub resolution: GroupResolution,
}

/// A group that still needs either model-assisted resolution or a user title.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingMediaGroup {
    pub root: String,
    pub group: MediaGroup,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MediaGroupKind {
    Movie,
    Series,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum GroupResolution {
    Pending,
    Matched {
        tmdb_id: u64,
        media_type: MetadataMediaType,
    },
    Ignored,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MetadataMediaType {
    Movie,
    Tv,
}

/// Strict JSON that a configured small model must return after inspecting a
/// filename batch. It is an intent, not a trusted match: TMDb candidates still
/// need selection/validation before anything becomes `Matched`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolverIntent {
    pub media_type: MetadataMediaType,
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub confidence_milli: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TmdbCandidate {
    pub tmdb_id: u64,
    pub media_type: MetadataMediaType,
    pub title: String,
    pub year: Option<u16>,
    pub overview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSelection {
    pub tmdb_id: u64,
    pub confidence_milli: u16,
}

/// User-configured, OpenAI-compatible endpoint. `api_key_env` is a process
/// environment-variable name, never the secret value itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelResolverConfig {
    pub base_url: String,
    pub model_id: String,
    pub api_key_env: String,
}

/// Connection settings used before a user selects a concrete model. This
/// excludes `model_id` because a compatible service can provide its list only
/// after the user explicitly connects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelDiscoveryConfig {
    pub base_url: String,
    pub api_key_env: String,
}

/// Safe user-facing result for a deliberate model-service connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelDiscoveryResult {
    pub connected: bool,
    pub models: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TmdbConfig {
    pub access_token_env: String,
    #[serde(default = "default_tmdb_language")]
    pub language: String,
}

/// The small model used to turn untrusted filenames into a TMDb lookup intent.
/// Users can either reuse a configured ACP Agent or configure a dedicated
/// OpenAI-compatible endpoint for this low-cost task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ResolverProviderConfig {
    AcpAgent {
        profile_id: String,
        profiles: AgentProfilesHint,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_effort: Option<String>,
    },
    DirectApi {
        model: ModelResolverConfig,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelDiscoveryConfig {
    pub profile_id: String,
    pub profiles: AgentProfilesHint,
}

pub type AgentModelDiscoveryResult = AcpModelDiscoveryResult;

fn default_tmdb_language() -> String {
    "zh-CN".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolverRunConfig {
    pub privacy_acknowledged: bool,
    pub provider: ResolverProviderConfig,
    pub tmdb: TmdbConfig,
}

/// A deliberately small, filename-free connectivity check for the saved
/// metadata credentials and selected model configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CredentialValidationConfig {
    pub provider: ResolverProviderConfig,
    pub tmdb: TmdbConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CredentialValidationResult {
    pub model: CredentialValidationItem,
    pub tmdb: CredentialValidationItem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CredentialValidationItem {
    pub verified: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolverPreview {
    pub intent: ResolverIntent,
    pub candidates: Vec<TmdbCandidate>,
    pub selection: Option<ResolverSelection>,
    pub can_auto_match: bool,
}

/// Current on-disk schema for `movie.json` / `series.json` / episode JSON.
pub const METADATA_SCHEMA_VERSION: u32 = 2;

/// Actor ↔ role pairing from TMDb credits (no image URLs — keep files small).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataCastMember {
    pub name: String,
    pub character: String,
    pub order: u32,
}

/// One durable TMDb-derived document. `kind` distinguishes series overview,
/// episode, and movie files while keeping future context loading uniform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredMetadata {
    pub schema_version: u32,
    pub kind: StoredMetadataKind,
    pub tmdb_id: u64,
    pub series_tmdb_id: Option<u64>,
    pub title: String,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    #[serde(default)]
    pub genres: Vec<String>,
    /// Main cast (series/movie) or guest stars (episode when available).
    #[serde(default)]
    pub cast: Vec<MetadataCastMember>,
    /// TV creators or movie directors.
    #[serde(default)]
    pub creators: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StoredMetadataKind {
    Series,
    Episode,
    Movie,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataWriteResult {
    pub root: String,
    pub group_key: String,
    pub tmdb_id: u64,
    pub media_type: MetadataMediaType,
    pub written_files: Vec<String>,
}

pub const WIKI_METADATA_SCHEMA_VERSION: u32 = 1;
/// Local wiki.json older than this is considered stale (30 days).
pub const WIKI_STALE_AFTER_MS: u128 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WikiMatchMethod {
    Wikidata,
    Search,
    UserSelected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WikiCandidateSource {
    Wikidata,
    Search,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiMatchInfo {
    pub method: WikiMatchMethod,
    pub candidates_considered: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiCharacter {
    pub name: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiEpisodeSummary {
    pub season: u32,
    pub episode: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub plot: String,
}

/// Optional Wikipedia enrichment stored beside TMDb JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiMetadata {
    pub schema_version: u32,
    pub wikidata_id: Option<String>,
    pub page_lang: String,
    pub page_title: String,
    pub page_url: String,
    pub extract: String,
    pub attribution: String,
    pub license: String,
    pub match_info: WikiMatchInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<WikiCharacter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub episodes: Vec<WikiEpisodeSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationships: Option<String>,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiEnrichmentCandidate {
    pub page_lang: String,
    pub page_title: String,
    pub page_url: String,
    pub wikidata_id: Option<String>,
    pub extract: Option<String>,
    pub source: WikiCandidateSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiZhReference {
    pub page_lang: String,
    pub page_title: String,
    pub page_url: String,
    pub extract: Option<String>,
    pub wikidata_id: Option<String>,
    pub aligned_with_en: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiGroupStatus {
    pub group_key: String,
    pub existing: Option<WikiMetadata>,
    pub is_stale: bool,
    pub stale_after_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TmdbGroupStatus {
    pub group_key: String,
    pub title: Option<String>,
    pub cast_count: u32,
    pub creators_count: u32,
    pub episode_file_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub updated_at_ms: u128,
}

/// Series-level metadata cached in MCP snapshot (no per-episode fields).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SeriesLibraryCache {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synopsis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characters: Option<Vec<WikiCharacter>>,
    #[serde(default)]
    pub creators: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_attribution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_page_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeIndexEntry {
    pub season: u32,
    pub episode: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiEnrichmentPreview {
    pub wikidata_candidate: Option<WikiEnrichmentCandidate>,
    pub search_candidates: Vec<WikiEnrichmentCandidate>,
    pub recommended: Option<WikiEnrichmentCandidate>,
    pub needs_user_pick: bool,
    pub conflict: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing: Option<WikiMetadata>,
    pub is_stale: bool,
    pub stale_after_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zhwiki_reference: Option<WikiZhReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiWriteResult {
    pub root: String,
    pub group_key: String,
    pub written_file: String,
}

/// Materialized read model: which field comes from TMDb vs Wikipedia.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MergedMediaContext {
    pub overview: Option<String>,
    pub synopsis: Option<String>,
    pub episode_overview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characters: Option<Vec<WikiCharacter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_episode_plot: Option<String>,
    pub wiki_attribution: Option<String>,
    pub wiki_page_url: Option<String>,
}

/// Trusted-by-app structure containing untrusted remote reference data. Future
/// Agent prompts must attach this as media context, never as system text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadataContext {
    pub media_path: String,
    pub group: StoredMetadata,
    pub item: Option<StoredMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki: Option<WikiMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged: Option<MergedMediaContext>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn provider_config_accepts_frontend_camel_case_fields() {
        let provider: ResolverProviderConfig = serde_json::from_value(json!({
            "kind": "acpAgent",
            "profileId": "codex",
            "profiles": { "activeProfileId": "codex", "profiles": [] },
            "modelId": "gpt-mini",
            "reasoningEffort": "low"
        }))
        .expect("deserialize frontend provider");
        match provider {
            ResolverProviderConfig::AcpAgent {
                profile_id,
                model_id,
                reasoning_effort,
                ..
            } => {
                assert_eq!(profile_id, "codex");
                assert_eq!(model_id.as_deref(), Some("gpt-mini"));
                assert_eq!(reasoning_effort.as_deref(), Some("low"));
            }
            ResolverProviderConfig::DirectApi { .. } => panic!("expected ACP provider"),
        }
    }
}
