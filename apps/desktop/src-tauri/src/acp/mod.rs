//! App-facing ACP exports; implementation lives in `lumina-acp` (M5).
//!
//! Chat snapshot lifecycle + MCP session environment live in `adapter`.

pub mod adapter;

pub use lumina_acp::{
    AcpClientSettings, AcpError, AcpErrorCode, AcpEvent, AcpService, AcpSessionModelOptions,
    AcpStatus, AgentKind, AgentProfileInput, AgentProfileStatus, AgentProfilesHint, PermissionMode,
    PermissionOption, SavedSessionHint, ThinkingLevel, VideoPromptContext,
};
