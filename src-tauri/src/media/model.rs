//! Media inspection result types (camelCase JSON for the frontend).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub format_name: Option<String>,
    pub format_long_name: Option<String>,
    pub duration_ms: Option<u64>,
    pub size_bytes: Option<u64>,
    pub bit_rate: Option<u64>,
    pub streams: Vec<MediaStream>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaStream {
    pub index: u32,
    pub kind: StreamKind,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bit_rate: Option<u64>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Data,
    Attachment,
    Unknown,
}

impl MediaInfo {
    pub fn primary_video(&self) -> Option<&MediaStream> {
        self.streams.iter().find(|s| s.kind == StreamKind::Video)
    }

    pub fn primary_audio(&self) -> Option<&MediaStream> {
        self.streams.iter().find(|s| s.kind == StreamKind::Audio)
    }
}
