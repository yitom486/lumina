//! Serializable local-library contracts. These are also the stable boundary for
//! future resolver tools; no model provider or TMDb transport is embedded here.

use serde::{Deserialize, Serialize};

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
    pub last_scan_at_ms: Option<u128>,
    pub indexed_files: usize,
    pub pending_groups: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TmdbConfig {
    pub access_token_env: String,
    #[serde(default = "default_tmdb_language")]
    pub language: String,
}

fn default_tmdb_language() -> String {
    "zh-CN".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolverRunConfig {
    pub privacy_acknowledged: bool,
    pub model: ModelResolverConfig,
    pub tmdb: TmdbConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolverPreview {
    pub intent: ResolverIntent,
    pub candidates: Vec<TmdbCandidate>,
    pub selection: Option<ResolverSelection>,
    pub can_auto_match: bool,
}
