//! Optional on-demand ACP Client (library crate). Not required for playback.
//!
//! Layout: `domain` (pure data) → `agent` (discovery/launch) → `wire`
//! (protocol codec, pure) → `runtime` (live processes) + `jobs` (isolated tasks).
//! One intentional exception: the `AcpService::prompt_isolated_restricted` /
//! `discover_isolated_models` thin forwarders in `runtime::service` call into
//! `jobs::isolated` (compat bridge, documented on each forwarder).

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
    AgentProfileInput, AgentProfileStatus, AgentProfilesHint, PermissionMode, PermissionOption,
    SavedSessionHint, SessionEnvironment, ThinkingLevel, VideoPromptContext,
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
    pub use crate::wire::codec::*;
    pub use crate::wire::permission::*;
    pub use crate::wire::sanitize::*;
    pub use crate::wire::session::{
        authenticate_params, initialize_params, initialize_params_restricted,
        parse_initialize_result, parse_session_id, parse_session_model_options, parse_stop_reason,
        session_cancel_params, session_close_params, session_new_params, session_resume_params,
        session_set_config_option_params, AuthMethod, InitializeResult,
    };
    // Deprecated wire shim (auth policy moved to `agent::launch`); re-exported
    // so the old `protocol::pick_auth_method` path still resolves.
    #[allow(deprecated)]
    pub use crate::wire::session::pick_auth_method;
    pub use crate::wire::updates::*;

    /// Legacy 2-arg shape (`session_id`, `text` only). Kept so pre-reorg
    /// `lumina_acp::protocol::session_prompt_params` callers still compile;
    /// `context`/`history_context` default to `None`. New code must call
    /// `crate::wire::session::session_prompt_params` directly.
    pub fn session_prompt_params(session_id: &str, text: &str) -> serde_json::Value {
        crate::wire::session::session_prompt_params(session_id, text, None, None)
    }
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

#[cfg(test)]
mod compat_tests {
    /// Locks the pre-reorg 2-arg `protocol::session_prompt_params` shape.
    #[test]
    fn legacy_protocol_session_prompt_params_shape() {
        let value = crate::protocol::session_prompt_params("s1", "hi");
        assert_eq!(value.get("sessionId").and_then(|v| v.as_str()), Some("s1"));
        let prompt = value
            .get("prompt")
            .and_then(|v| v.as_array())
            .expect("prompt");
        assert_eq!(prompt.len(), 1);
        assert_eq!(prompt[0].get("text").and_then(|v| v.as_str()), Some("hi"));
    }
}
