//! Isolated ACP tasks with purpose-specific session kinds and settings.
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

/// Fixed settings for a chapter analysis session. Chapter sessions may use
/// Lumina MCP tools and must expose vision capability.
pub(crate) fn chapter_client_settings() -> AcpClientSettings {
    AcpClientSettings {
        permission_mode: PermissionMode::Ask,
        thinking_level: ThinkingLevel::Hidden,
        agent_mode: "chapter".into(),
        vision_capable: true,
        model_id: None,
        reasoning_effort: None,
    }
}

/// A reusable, independent ACP session for one chapter-segmentation task.
///
/// The session deliberately owns its own `AcpService`: it never shares the
/// interactive chat service and it never receives a saved-session hint. The
/// first prompt can contain the full chapter request; later prompts can carry
/// only the validation delta while keeping the same ACP session alive.
pub struct ChapterSession {
    service: AcpService,
    cwd: Option<String>,
    profile_id: String,
    profiles: AgentProfilesHint,
    model_selection: Option<AcpSessionModelSelection>,
    task_label: Option<String>,
}

impl ChapterSession {
    /// Construct a fresh chapter session without starting an Agent process.
    /// The process is started lazily by the first call to [`Self::prompt`].
    pub fn new(
        cwd: Option<String>,
        profile_id: String,
        profiles: AgentProfilesHint,
        model_selection: Option<AcpSessionModelSelection>,
        task_label: Option<String>,
    ) -> Self {
        let service = AcpService::new();
        if let Ok(mut selection) = service.next_session_model_selection.lock() {
            *selection = model_selection.clone();
        }
        Self {
            service,
            cwd,
            profile_id,
            profiles,
            model_selection,
            task_label,
        }
    }

    /// Send one prompt in this chapter session.
    ///
    /// No saved session is supplied, so the ACP session is independent from
    /// persisted chat history. Repeated calls reuse the same live
    /// `AcpService` session unless ACP reports a transport failure, in which
    /// case the existing prompt flow returns its business error unchanged.
    pub fn prompt(&self, text: impl AsRef<str>) -> Result<String, AcpError> {
        // Re-arm the selection so a subsequent prompt can still apply it if a
        // transport failure caused ACP to drop the live session before the
        // worker decides whether to continue.
        if let Ok(mut selection) = self.service.next_session_model_selection.lock() {
            *selection = self.model_selection.clone();
        }
        self.service.prompt_with_label(
            text,
            self.cwd.clone(),
            Some(self.profile_id.clone()),
            None,
            &[],
            None,
            chapter_client_settings(),
            self.profiles.clone(),
            SessionKind::Chapter,
            |_| {},
            self.task_label.as_deref(),
        )
    }

    /// End the chapter Agent process and discard its live ACP session.
    pub fn close(&self) -> Result<(), AcpError> {
        self.service.close_session()
    }
}

impl Drop for ChapterSession {
    fn drop(&mut self) {
        self.service.close_session_for_shutdown();
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
        &[],
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

/// Run one chapter prompt in a fresh ACP process/session, then close it.
/// The caller supplies a chapter-owned cwd so the host can isolate its
/// workspace/snapshot; no saved chat session or chat prompt context is used.
pub fn prompt_isolated_chapter(
    text: impl AsRef<str>,
    cwd: Option<String>,
    profile_id: String,
    profiles: AgentProfilesHint,
    model_selection: Option<AcpSessionModelSelection>,
    task_label: Option<String>,
) -> Result<String, AcpError> {
    let session = ChapterSession::new(cwd, profile_id, profiles, model_selection, task_label);
    let outcome = session.prompt(text);
    let _ = session.close();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_settings_remain_tool_free_and_blind() {
        let settings = isolated_client_settings();
        assert!(!settings.vision_capable);
        assert_eq!(settings.agent_mode, "subtitle-workshop");
    }

    #[test]
    fn chapter_settings_enable_vision() {
        let settings = chapter_client_settings();
        assert!(settings.vision_capable);
        assert_eq!(settings.agent_mode, "chapter");
    }

    fn test_profiles() -> AgentProfilesHint {
        AgentProfilesHint {
            active_profile_id: "chapter-agent".into(),
            profiles: Vec::new(),
        }
    }

    #[test]
    fn chapter_session_preserves_constructor_configuration() {
        let model_selection = Some(AcpSessionModelSelection {
            model_id: "chapter-model".into(),
            reasoning_effort: Some("low".into()),
        });
        let profiles = test_profiles();
        let session = ChapterSession::new(
            Some("C:/lumina/chapter".into()),
            "chapter-agent".into(),
            profiles.clone(),
            model_selection.clone(),
            Some("episode-1-attempt-1".into()),
        );

        assert_eq!(session.cwd.as_deref(), Some("C:/lumina/chapter"));
        assert_eq!(session.profile_id, "chapter-agent");
        assert_eq!(session.profiles, profiles);
        assert_eq!(session.model_selection, model_selection);
        assert_eq!(session.task_label.as_deref(), Some("episode-1-attempt-1"));
    }

    #[test]
    fn chapter_session_close_is_idempotent_without_starting_agent() {
        let session =
            ChapterSession::new(None, "chapter-agent".into(), test_profiles(), None, None);

        assert!(session.close().is_ok());
        assert!(session.close().is_ok());
        assert!(!session.service.status(&test_profiles()).session_active);
    }
}
