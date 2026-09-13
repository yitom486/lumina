//! App-side library adapters (M7).
//!
//! Agent-backed resolver/discovery orchestration that needs ACP session
//! access lives here. The crate only sees the core invoker port plus an
//! opaque profiles payload.

use lumina_acp::{AcpService, AgentProfilesHint};

use crate::acp::adapter::AcpAgentInvoker;
use crate::library::{
    AgentModelDiscoveryResult, AgentModelOption, AgentModelOptions, LibraryError,
    ResolverProviderConfig,
};

/// Parse the opaque profiles payload from library configs into the ACP hint.
/// Unparseable payloads map to the same not-configured error the crate used
/// to produce for an empty selection.
pub fn parse_agent_profiles(value: &serde_json::Value) -> Result<AgentProfilesHint, LibraryError> {
    serde_json::from_value(value.clone())
        .map_err(|_| LibraryError::resolver_not_configured(Some("ACP profile selection is empty")))
}

/// Non-empty profile selection, mirroring the crate-side guard.
pub fn require_agent_profiles(
    value: &serde_json::Value,
) -> Result<AgentProfilesHint, LibraryError> {
    let profiles = parse_agent_profiles(value)?;
    if profiles.profiles.is_empty() {
        return Err(LibraryError::resolver_not_configured(Some(
            "ACP profile selection is empty",
        )));
    }
    Ok(profiles)
}

/// Isolated resolver invoker bound to an explicit profile selection.
pub fn resolver_invoker(profiles: AgentProfilesHint) -> AcpAgentInvoker {
    AcpAgentInvoker::new(profiles)
}

/// Build the resolver invoker for a provider config. Direct API providers get
/// an empty selection that is never invoked; ACP providers go through the
/// same empty-selection guard the crate used to enforce.
pub fn resolver_invoker_for_provider(
    provider: &ResolverProviderConfig,
) -> Result<AcpAgentInvoker, LibraryError> {
    match provider {
        ResolverProviderConfig::AcpAgent { profiles, .. } => {
            Ok(resolver_invoker(require_agent_profiles(profiles)?))
        }
        ResolverProviderConfig::DirectApi { .. } => Ok(resolver_invoker(empty_hint())),
    }
}

/// Lenient invoker for credential validation. The validation flow reports
/// per-service failure items instead of typed errors, so an empty selection
/// falls back to a hint that can never successfully invoke (the pre-invoke
/// guard rejects it before any spawn, exactly like the crate-side guard did).
pub fn validation_invoker(provider: &ResolverProviderConfig) -> AcpAgentInvoker {
    resolver_invoker_for_provider(provider).unwrap_or_else(|_| resolver_invoker(empty_hint()))
}

fn empty_hint() -> AgentProfilesHint {
    AgentProfilesHint {
        active_profile_id: String::new(),
        profiles: Vec::new(),
    }
}

/// Short-lived agent model discovery. Mirrors the removed
/// `library::discover_agent_models`; errors unchanged.
/// A missing/empty selection flows into the agent call exactly as before and
/// surfaces as the generic connection-failure result, not a typed error.
pub fn discover_agent_models(
    profile_id: String,
    profiles: &serde_json::Value,
) -> AgentModelDiscoveryResult {
    let profiles = parse_agent_profiles(profiles).unwrap_or(AgentProfilesHint {
        active_profile_id: String::new(),
        profiles: Vec::new(),
    });
    match AcpService::discover_isolated_models(profile_id, profiles) {
        Ok(result) => AgentModelDiscoveryResult {
            connected: result.connected,
            options: AgentModelOptions {
                models: result
                    .options
                    .models
                    .into_iter()
                    .map(|option| AgentModelOption {
                        value: option.value,
                        name: option.name,
                        description: option.description,
                    })
                    .collect(),
                reasoning_efforts: result
                    .options
                    .reasoning_efforts
                    .into_iter()
                    .map(|option| AgentModelOption {
                        value: option.value,
                        name: option.name,
                        description: option.description,
                    })
                    .collect(),
                current_model_id: result.options.current_model_id,
                current_reasoning_effort: result.options.current_reasoning_effort,
            },
            message: result.message,
        },
        Err(error) => {
            tracing::warn!(details = %error, "agent model discovery failed");
            AgentModelDiscoveryResult {
                connected: false,
                options: AgentModelOptions::default(),
                message: "无法连接 Agent，请检查 Agent 配置或登录状态后重试".into(),
            }
        }
    }
}
