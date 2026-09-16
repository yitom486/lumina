//! Optional on-demand ACP Client (library crate). Not required for playback.
//!
//! Layout: `domain` (pure data) → `agent` (discovery/launch) → `wire`
//! (protocol codec, pure) → `runtime` (live processes) + `jobs` (isolated tasks).
//! Isolated tasks live under `jobs::isolated`; interactive sessions live under
//! `runtime::service`.

pub mod agent;
pub mod domain;
pub mod error;
pub mod jobs;
pub mod runtime;
pub mod wire;

// ---- Canonical root API (stable; app/Tauri use only these) ----
pub use agent::AgentProfile;
pub use domain::{
    set_default_environment, AcpClientSettings, AcpEvent, AcpModelDiscoveryResult,
    AcpSessionModelOptions, AcpSessionModelSelection, AcpSessionOption, AcpStatus, AgentKind,
    AgentProfileInput, AgentProfileStatus, AgentProfilesHint, AgentSessionInfo,
    AgentSessionListResult, PermissionMode, PermissionOption, SavedSessionHint, SessionEnvironment,
    SessionKind, ThinkingLevel, VideoPromptContext,
};
pub use error::{AcpError, AcpErrorCode};
pub use jobs::pool::{PoolConfig, WorkshopPool, DEFAULT_POOL_SIZE};
pub use runtime::service::AcpService;
