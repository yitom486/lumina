//! Explicit MCP tool profiles (M6).
//!
//! Previously the tool set was driven only by scattered snapshot capability
//! booleans, and `tools/call` trusted whatever name the client sent.
//! Now [`McpToolProfile`]/[`ToolPolicy`] own the central tool directory:
//! `tools/list` advertises it and `tools/call` enforces it, so a hand-written
//! name for an unauthorized tool is rejected instead of executed.
//!
//! Snapshot booleans still describe a Chat session (`AgentCapabilities`);
//! profiles decide which slice of the directory applies. Tool JSON schemas
//! and handler logic are unchanged.

use super::snapshot::LuminaMcpSnapshot;
use super::tools::{subtitle_workshop_enabled, video_annotations_enabled, vision_capable};

// Canonical tool identities live in `lumina-core` (transport-free); this
// module keeps the MCP-side re-export so existing `policy::TOOL_*` paths and
// `tools/list` order stay byte-identical.
pub use lumina_core::tool_contract::{
    TOOL_AUDIO_MARKS, TOOL_CAPTURE_CHAPTER_EVIDENCE, TOOL_CAPTURE_FRAMES,
    TOOL_CREATE_CHAPTER_OUTLINE, TOOL_ENTITY_TIMELINE, TOOL_EPISODE_INDEX, TOOL_EPISODE_TRANSCRIPT,
    TOOL_FINALIZE_CHAPTER_TASK, TOOL_LIBRARY_CONTEXT, TOOL_PLAYBACK_CONTEXT,
    TOOL_PROPOSE_ANNOTATION, TOOL_SEEK_PLAYBACK, TOOL_SUBTITLE_CUES, TOOL_TRANSCRIPT_WINDOW,
    TOOL_UPDATE_CHAPTER_DRAFT, TOOL_WRITE_SUBTITLE_TRACK,
};

/// Env var carrying the profile into the MCP server process.
pub const TOOL_PROFILE_ENV: &str = "LUMINA_MCP_TOOL_PROFILE";

/// Which tool set an agent session/task may see and call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpToolProfile {
    /// Long-lived chat session: existing capabilities (default).
    #[default]
    Chat,
    /// Subtitle workshop entry: subtitle read/write tools only.
    SubtitleWorkshop,
    /// Constrained metadata task: no Lumina MCP tools by default.
    MetadataResolver,
    /// Explicitly tool-free session/task.
    NoTools,
}

impl McpToolProfile {
    pub fn env_value(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::SubtitleWorkshop => "subtitle-workshop",
            Self::MetadataResolver => "metadata-resolver",
            Self::NoTools => "no-tools",
        }
    }

    pub fn from_env_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "subtitle-workshop" | "subtitle_workshop" | "workshop" => Self::SubtitleWorkshop,
            "metadata-resolver" | "metadata_resolver" | "resolver" => Self::MetadataResolver,
            "no-tools" | "no_tools" | "notools" | "none" => Self::NoTools,
            _ => Self::Chat,
        }
    }
}

/// Resolve the serving profile from the process environment.
/// Unset or unknown values fall back to [`McpToolProfile::Chat`],
/// preserving pre-policy behavior.
pub fn tool_profile_from_env() -> McpToolProfile {
    McpToolProfile::from_env_value(&std::env::var(TOOL_PROFILE_ENV).unwrap_or_default())
}

// Central tool directory (canonical names in `lumina-core`; `tool_json` in
// `server.rs` and the `handle_tool_call` dispatch in `tools.rs` consume them).
/// Tools visible under `profile`, in `tools/list` order.
pub fn allowed_tool_names(
    profile: McpToolProfile,
    snapshot: &LuminaMcpSnapshot,
) -> Vec<&'static str> {
    allowed_tool_names_for_chapter_task(profile, snapshot, false)
}

pub fn allowed_tool_names_for_chapter_task(
    profile: McpToolProfile,
    snapshot: &LuminaMcpSnapshot,
    chapter_task_enabled: bool,
) -> Vec<&'static str> {
    match profile {
        McpToolProfile::MetadataResolver | McpToolProfile::NoTools => Vec::new(),
        McpToolProfile::SubtitleWorkshop => {
            vec![TOOL_SUBTITLE_CUES, TOOL_WRITE_SUBTITLE_TRACK]
        }
        McpToolProfile::Chat => {
            let mut tools = vec![
                TOOL_PLAYBACK_CONTEXT,
                TOOL_SEEK_PLAYBACK,
                TOOL_LIBRARY_CONTEXT,
                TOOL_EPISODE_INDEX,
                TOOL_ENTITY_TIMELINE,
                TOOL_TRANSCRIPT_WINDOW,
                TOOL_EPISODE_TRANSCRIPT,
                TOOL_AUDIO_MARKS,
            ];
            if subtitle_workshop_enabled(snapshot) {
                tools.push(TOOL_SUBTITLE_CUES);
                tools.push(TOOL_WRITE_SUBTITLE_TRACK);
            }
            if vision_capable(snapshot) {
                tools.push(TOOL_CAPTURE_FRAMES);
            }
            if video_annotations_enabled(snapshot) {
                tools.push(TOOL_PROPOSE_ANNOTATION);
            }
            if chapter_task_enabled {
                tools.push(TOOL_CREATE_CHAPTER_OUTLINE);
                tools.push(TOOL_CAPTURE_CHAPTER_EVIDENCE);
                tools.push(TOOL_UPDATE_CHAPTER_DRAFT);
                tools.push(TOOL_FINALIZE_CHAPTER_TASK);
            }
            tools
        }
    }
}

fn is_known_tool(name: &str) -> bool {
    lumina_core::tool_contract::is_known_tool(name)
}

/// Policy wrapper used by both `tools/list` and `tools/call`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolPolicy {
    pub profile: McpToolProfile,
    pub chapter_task_enabled: bool,
}

impl ToolPolicy {
    pub fn new(profile: McpToolProfile) -> Self {
        Self {
            profile,
            chapter_task_enabled: false,
        }
    }

    pub fn for_chapter_task(profile: McpToolProfile) -> Self {
        Self {
            profile,
            chapter_task_enabled: true,
        }
    }

    pub fn tools(&self, snapshot: &LuminaMcpSnapshot) -> Vec<&'static str> {
        if self.chapter_task_enabled {
            allowed_tool_names_for_chapter_task(self.profile, snapshot, true)
        } else {
            allowed_tool_names(self.profile, snapshot)
        }
    }

    /// Authorize one `tools/call`. Unknown names keep the historical
    /// `Unknown tool` error; known-but-unauthorized names are rejected.
    pub fn check(&self, snapshot: &LuminaMcpSnapshot, name: &str) -> Result<(), String> {
        if self.tools(snapshot).contains(&name) {
            return Ok(());
        }
        if is_known_tool(name) {
            return Err("该工具未对当前任务开放".to_string());
        }
        Err(format!("Unknown tool: {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{AgentCapabilities, LuminaMcpSnapshot, SNAPSHOT_SCHEMA_VERSION};

    fn snapshot_with(vision: bool, workshop: bool, annotations: bool) -> LuminaMcpSnapshot {
        LuminaMcpSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            anchor: None,
            current_episode: None,
            library: None,
            session: None,
            capabilities: Some(AgentCapabilities {
                vision_capable: vision,
                subtitle_workshop_enabled: workshop,
                video_annotations_enabled: annotations,
            }),
            online: None,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn profile_parses_env_values_with_chat_default() {
        assert_eq!(McpToolProfile::from_env_value("chat"), McpToolProfile::Chat);
        assert_eq!(
            McpToolProfile::from_env_value("subtitle-workshop"),
            McpToolProfile::SubtitleWorkshop
        );
        assert_eq!(
            McpToolProfile::from_env_value("metadata-resolver"),
            McpToolProfile::MetadataResolver
        );
        assert_eq!(
            McpToolProfile::from_env_value("no-tools"),
            McpToolProfile::NoTools
        );
        assert_eq!(McpToolProfile::from_env_value(""), McpToolProfile::Chat);
        assert_eq!(
            McpToolProfile::from_env_value("something-new"),
            McpToolProfile::Chat
        );
    }

    #[test]
    fn chat_keeps_existing_capability_behavior() {
        let plain = snapshot_with(false, false, true);
        let names = allowed_tool_names(McpToolProfile::Chat, &plain);
        assert_eq!(
            names,
            vec![
                TOOL_PLAYBACK_CONTEXT,
                TOOL_SEEK_PLAYBACK,
                TOOL_LIBRARY_CONTEXT,
                TOOL_EPISODE_INDEX,
                TOOL_ENTITY_TIMELINE,
                TOOL_TRANSCRIPT_WINDOW,
                TOOL_EPISODE_TRANSCRIPT,
                TOOL_AUDIO_MARKS,
                TOOL_PROPOSE_ANNOTATION,
            ]
        );
        let full = snapshot_with(true, true, true);
        let names = allowed_tool_names(McpToolProfile::Chat, &full);
        assert!(names.contains(&TOOL_SUBTITLE_CUES));
        assert!(names.contains(&TOOL_WRITE_SUBTITLE_TRACK));
        assert!(names.contains(&TOOL_CAPTURE_FRAMES));
    }

    #[test]
    fn chapter_tools_require_the_explicit_task_scope() {
        let snapshot = snapshot_with(true, true, true);
        let ordinary = allowed_tool_names(McpToolProfile::Chat, &snapshot);
        assert!(!ordinary.contains(&TOOL_CREATE_CHAPTER_OUTLINE));
        let scoped = allowed_tool_names_for_chapter_task(McpToolProfile::Chat, &snapshot, true);
        assert!(scoped.contains(&TOOL_CREATE_CHAPTER_OUTLINE));
        assert!(scoped.contains(&TOOL_CAPTURE_CHAPTER_EVIDENCE));
        assert!(scoped.contains(&TOOL_UPDATE_CHAPTER_DRAFT));
        assert!(scoped.contains(&TOOL_FINALIZE_CHAPTER_TASK));
        assert!(
            allowed_tool_names_for_chapter_task(McpToolProfile::NoTools, &snapshot, true)
                .is_empty()
        );
    }

    #[test]
    fn workshop_lists_only_subtitle_tools() {
        let snapshot = snapshot_with(true, true, true);
        assert_eq!(
            allowed_tool_names(McpToolProfile::SubtitleWorkshop, &snapshot),
            vec![TOOL_SUBTITLE_CUES, TOOL_WRITE_SUBTITLE_TRACK]
        );
    }

    #[test]
    fn resolver_and_no_tools_list_nothing() {
        let snapshot = snapshot_with(true, true, true);
        assert!(allowed_tool_names(McpToolProfile::MetadataResolver, &snapshot).is_empty());
        assert!(allowed_tool_names(McpToolProfile::NoTools, &snapshot).is_empty());
    }

    #[test]
    fn unauthorized_call_is_rejected() {
        let snapshot = snapshot_with(false, false, true);
        let chat = ToolPolicy::new(McpToolProfile::Chat);
        assert!(chat.check(&snapshot, TOOL_TRANSCRIPT_WINDOW).is_ok());
        // Hand-written workshop name on a non-workshop snapshot.
        assert!(chat.check(&snapshot, TOOL_WRITE_SUBTITLE_TRACK).is_err());
        // Vision-gated tool without vision.
        assert!(chat.check(&snapshot, TOOL_CAPTURE_FRAMES).is_err());
        let none = ToolPolicy::new(McpToolProfile::NoTools);
        assert!(none.check(&snapshot, TOOL_PLAYBACK_CONTEXT).is_err());
        // Unknown names keep the historical error.
        let err = none
            .check(&snapshot, "lumina_do_anything")
            .expect_err("unknown");
        assert!(err.contains("Unknown tool"));
    }
}
