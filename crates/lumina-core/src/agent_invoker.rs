//! Agent invocation port for AI tasks (translation, polishing).
//!
//! AI crates never implement ACP or model APIs. The app provides the
//! ACP-backed implementation; tasks stay short-lived and data-isolated.
//! Home is `lumina-core` so every domain shares one port (M5 follow-up).

/// One isolated, data-only Agent call. No chat history, no MCP tools.
#[derive(Debug, Clone)]
pub struct IsolatedAgentTask {
    pub prompt: String,
    pub profile_id: String,
    pub model_id: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AgentTaskError {
    NotConfigured { details: Option<String> },
    Failed { details: Option<String> },
}

impl AgentTaskError {
    pub fn details(&self) -> Option<&str> {
        match self {
            Self::NotConfigured { details } | Self::Failed { details } => details.as_deref(),
        }
    }
}

pub trait AgentInvoker: Send + Sync {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError>;
}
