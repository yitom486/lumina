//! Client agent preferences passed from the frontend (Zustand persist); not stored in Rust.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Auto,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThinkingLevel {
    Hidden,
    Minimal,
    Verbose,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpClientSettings {
    #[serde(default = "default_permission_mode")]
    pub permission_mode: PermissionMode,
    #[serde(default = "default_thinking_level")]
    pub thinking_level: ThinkingLevel,
    #[serde(default = "default_agent_mode")]
    pub agent_mode: String,
    #[serde(default)]
    pub vision_capable: bool,
    /// Optional override applied on connect / new chat / live session update.
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

impl AcpClientSettings {
    pub fn model_selection(&self) -> Option<crate::acp::AcpSessionModelSelection> {
        let model_id = self.model_id.as_deref()?.trim();
        if model_id.is_empty() {
            return None;
        }
        Some(crate::acp::AcpSessionModelSelection {
            model_id: model_id.to_string(),
            reasoning_effort: self
                .reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        })
    }
}

impl Default for AcpClientSettings {
    fn default() -> Self {
        Self {
            permission_mode: default_permission_mode(),
            thinking_level: default_thinking_level(),
            agent_mode: default_agent_mode(),
            vision_capable: false,
            model_id: None,
            reasoning_effort: None,
        }
    }
}

fn default_permission_mode() -> PermissionMode {
    PermissionMode::Auto
}

fn default_thinking_level() -> ThinkingLevel {
    ThinkingLevel::Minimal
}

fn default_agent_mode() -> String {
    "default".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_selection_skips_empty_model_id() {
        let settings = AcpClientSettings {
            model_id: Some("".into()),
            ..Default::default()
        };
        assert!(settings.model_selection().is_none());
    }

    #[test]
    fn model_selection_trims_values() {
        let settings = AcpClientSettings {
            model_id: Some(" gpt-5 ".into()),
            reasoning_effort: Some(" high ".into()),
            ..Default::default()
        };
        let selection = settings.model_selection().expect("selection");
        assert_eq!(selection.model_id, "gpt-5");
        assert_eq!(selection.reasoning_effort.as_deref(), Some("high"));
    }
}
