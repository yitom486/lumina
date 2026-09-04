//! Status / resolve / install event models for yt-dlp.

use serde::{Deserialize, Serialize};

use crate::media::model::MediaChapter;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlStatus {
    pub available: bool,
    pub cli_ready: bool,
    pub cli_path: Option<String>,
    pub version: Option<String>,
    pub install_supported: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlFormat {
    pub format_id: String,
    pub ext: Option<String>,
    pub height: Option<u32>,
    pub width: Option<u32>,
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub tbr: Option<f64>,
    pub format_note: Option<String>,
    /// Direct media URL from resolver — for player only; never put in Agent snapshot.
    #[serde(default, skip_serializing)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct YtdlSubtitleTrack {
    pub language: String,
    pub ext: Option<String>,
    pub name: Option<String>,
    /// Signed subtitle resource URL. In-process only; stripped before IPC/snapshot.
    #[serde(default, skip_serializing)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlResolveResult {
    pub media_id: String,
    pub title: Option<String>,
    pub duration_ms: Option<u64>,
    pub webpage_url: Option<String>,
    pub extractor: Option<String>,
    pub chapters: Vec<MediaChapter>,
    pub formats: Vec<YtdlFormat>,
    pub subtitles: Vec<YtdlSubtitleTrack>,
    /// Best effort playable URL (may be dash video-only — Step 3 picks format).
    pub recommended_url: Option<String>,
    pub recommended_format_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "PascalCase",
    rename_all_fields = "camelCase"
)]
pub enum YtdlInstallEvent {
    Progress {
        stage: String,
        message: String,
        downloaded: Option<u64>,
        total: Option<u64>,
    },
    Finished {
        status: YtdlStatus,
    },
}
