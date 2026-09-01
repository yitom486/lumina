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
}

impl Default for AcpClientSettings {
    fn default() -> Self {
        Self {
            permission_mode: default_permission_mode(),
            thinking_level: default_thinking_level(),
            agent_mode: default_agent_mode(),
            vision_capable: false,
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
