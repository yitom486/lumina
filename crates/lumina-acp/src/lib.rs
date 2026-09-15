//! Optional on-demand ACP Client (library crate). Not required for playback.
//!
//! Layout: `domain` (pure data) → `agent` (discovery/launch) → `wire`
//! (protocol codec, pure) → `runtime` (live processes) + `jobs` (isolated tasks).

pub mod agent;
pub mod domain;
pub mod error;
pub mod jobs;
pub mod runtime;
pub mod wire;

// ---- Canonical root API (stable; app/Tauri use only these) ----
pub use agent::{AgentKind, AgentProfile, AgentProfileStatus};
pub use domain::{
    set_default_environment, AcpClientSettings, AcpEvent, AcpModelDiscoveryResult,
    AcpSessionModelOptions, AcpSessionModelSelection, AcpSessionOption, AcpStatus,
    AgentProfileInput, AgentProfilesHint, PermissionMode, PermissionOption, SavedSessionHint,
    SessionEnvironment, ThinkingLevel, VideoPromptContext,
};
pub use error::{AcpError, AcpErrorCode};
pub use jobs::pool::{IsolatedSessionPool, PoolConfig, WorkshopPool, DEFAULT_POOL_SIZE};
pub use runtime::service::AcpService;

// ---- Compatibility shims for pre-reorg paths ----
// Old flat `lumina_acp::{service,protocol,...}` paths keep resolving so
// `apps/desktop/src-tauri` (`crate::acp::paths/settings/...` via
// `pub use lumina_acp::*`) needs no churn. New code must use the
// `domain/agent/wire/runtime/jobs` hierarchy above.

pub mod context {
    pub use crate::domain::context::*;
}

pub mod model {
    pub use crate::domain::model::*;
}

pub mod settings {
    pub use crate::domain::settings::*;
}

pub mod environment {
    pub use crate::domain::environment::*;
}

pub mod discover {
    pub use crate::agent::discover::*;
}

pub mod profile {
    pub use crate::agent::launch::*;
    pub use crate::agent::profile::*;
}

pub mod paths {
    pub use crate::agent::status::*;
    pub use crate::agent::workspace::*;
}

pub mod protocol {
    pub use crate::wire::*;
}

pub mod service {
    pub use crate::runtime::service::*;
}

pub mod host {
    pub use crate::runtime::host::*;
}

pub mod workshop {
    pub use crate::jobs::pool::*;
}

pub mod agent_reply_collector {
    pub use crate::jobs::collector::*;
}
