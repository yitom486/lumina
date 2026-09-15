//! Dynamic video context for `session/prompt` (pure data only).
//!
//! Structured metadata is **not** inlined into the prompt. Lumina writes
//! `.lumina/agent-context.json` and registers an MCP server so the Agent can
//! fetch playback/library context on demand. Stable tool-use guidance is
//! delivered once by the MCP server's `initialize.instructions` field.
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
    pub chapter_title: Option<String>,
    pub subtitle_choice_id: Option<String>,
    pub notes_excerpt: Option<String>,
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
                .chapter_title
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            && self
                .subtitle_choice_id
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
            && self
                .notes_excerpt
                .as_ref()
                .is_none_or(|s| s.trim().is_empty())
    }
}

pub fn snapshot_display_path(cwd: &Path) -> PathBuf {
    cwd.join(SNAPSHOT_RELATIVE_PATH)
}

// Deprecated wire shims: kept so `crate::acp::context::*` (via
// `pub use lumina_acp::*`) keeps resolving. New code must use
// `crate::wire::session::*`.
#[deprecated(note = "use crate::wire::session::session_prompt_params instead")]
pub use crate::wire::session::session_prompt_params;

#[deprecated(note = "use crate::wire::session::path_to_file_uri instead")]
pub use crate::wire::session::path_to_file_uri;

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
