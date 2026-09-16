//! Dynamic video context for `session/prompt` (pure data only).
//!
//! Progress (and episode plot **only on media switch**) are inlined into the
//! prompt. Richer metadata lives in `.lumina/agent-context.json` for MCP
//! on-demand fetch. Stable tool-use guidance is delivered once by the MCP
//! server's `initialize.instructions` field.
//!
//! Wire construction (`session_prompt_params`) lives in
//! `crate::wire::session`; this module keeps the [`VideoPromptContext`] DTO
//! and snapshot path helpers free of ACP JSON.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Mirrors `mcp::SNAPSHOT_RELATIVE_PATH`; acp must not depend on mcp (M5).
const SNAPSHOT_RELATIVE_PATH: &str = ".lumina/agent-context.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VideoPromptContext {
    pub media_path: Option<String>,
    pub media_title: Option<String>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub subtitle_choice_id: Option<String>,
    /// Filled by the app from local episode metadata before ACP prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_overview: Option<String>,
}

impl VideoPromptContext {
    pub fn is_empty(&self) -> bool {
        self.media_path.as_ref().is_none_or(|s| s.trim().is_empty())
            && self
                .media_title
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            && self.position_ms.is_none()
            && self.duration_ms.is_none()
            && self
                .subtitle_choice_id
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            && self.season.is_none()
            && self.episode.is_none()
            && self
                .episode_title
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            && self
                .episode_overview
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
    }
}

pub fn snapshot_display_path(cwd: &Path) -> PathBuf {
    cwd.join(SNAPSHOT_RELATIVE_PATH)
}

#[cfg(test)]
fn format_time_ms(ms: u64) -> String {
    let total_sec = ms / 1000;
    let h = total_sec / 3600;
    let m = (total_sec % 3600) / 60;
    let s = total_sec % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_ms_helpers() {
        assert_eq!(format_time_ms(83_000), "1:23");
    }
}
