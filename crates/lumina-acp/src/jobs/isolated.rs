//! Isolated workshop tasks: tool-free, chat-state-blind client settings.
//!
//! Pure move from `runtime/service.rs` (no behavior change).

use std::sync::atomic::Ordering;

use crate::agent::profile::prepare_profiles;
use crate::domain::model::{
    AcpModelDiscoveryResult, AcpSessionModelSelection, AgentProfilesHint, SessionKind,
};
use crate::domain::settings::{AcpClientSettings, PermissionMode, ThinkingLevel};
use crate::error::AcpError;
use crate::runtime::service::AcpService;

/// Fixed client settings for isolated workshop prompts (tool-free, blind
/// to chat state). Shared by the one-shot path and the P2 pool runner so
/// the two never drift apart.
pub(crate) fn isolated_client_settings() -> AcpClientSettings {
    AcpClientSettings {
        permission_mode: PermissionMode::Ask,
        thinking_level: ThinkingLevel::Hidden,
        agent_mode: "subtitle-workshop".into(),
        vision_capable: false,
        model_id: None,
        reasoning_effort: None,
    }
}

/// Run one prompt in a fresh ACP process/session using an existing profile,
/// then close it. This is deliberately separate from the interactive chat
/// service: no saved session, no chat context, and no Agent tool access.
pub fn prompt_isolated_restricted(
    text: impl AsRef<str>,
    profile_id: String,
    profiles: AgentProfilesHint,
    model_selection: Option<AcpSessionModelSelection>,
    task_label: Option<String>,
) -> Result<String, AcpError> {
    let service = AcpService::new();
    service.tool_access_enabled.store(false, Ordering::SeqCst);
    if let Ok(mut selection) = service.next_session_model_selection.lock() {
        *selection = model_selection;
    }
    let outcome = service.prompt_with_label(
        text,
        None,
        Some(profile_id),
        None,
        None,
        isolated_client_settings(),
        profiles,
        SessionKind::Workshop,
        |_| {},
        task_label.as_deref(),
    );
    let _ = service.close_session();
    outcome
}

/// Explicitly opens a short-lived, tool-disabled session and returns only
/// its model configuration options. No user/media data is sent.
pub fn discover_isolated_models(
    profile_id: String,
    profiles: AgentProfilesHint,
) -> Result<AcpModelDiscoveryResult, AcpError> {
    let service = AcpService::new();
    service.tool_access_enabled.store(false, Ordering::SeqCst);
    let prepared = prepare_profiles(&profiles);
    let session = service.spawn_session(
        None,
        None,
        &prepared,
        Some(&profile_id),
        true,
        SessionKind::Workshop,
        &mut |_| {},
    )?;
    let options = session.model_options.clone();
    if let Ok(mut guard) = service.session.lock() {
        *guard = Some(session);
    }
    let _ = service.close_session();
    Ok(AcpModelDiscoveryResult {
        connected: true,
        message: if options.models.is_empty() {
            "Agent 已连接，但未提供可选择的模型；将使用 Agent 默认模型".into()
        } else {
            "Agent 已连接，请选择用于媒体匹配的低成本模型与推理强度".into()
        },
        options,
    })
}
