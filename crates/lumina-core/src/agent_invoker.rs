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
    /// Stable diagnostic label owned by the caller (for example a batch id).
    /// It is metadata only and must never contain prompt or subtitle content.
    pub task_label: Option<String>,
    /// Precomputed label for the transport layer's identical retry send
    /// (for example `transport_attempt=2`). The caller builds both labels
    /// from one structured source; the transport layer never parses label
    /// strings. `None` reuses `task_label` for the retry send.
    pub retry_task_label: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AgentTaskError {
    NotConfigured {
        details: Option<String>,
    },
    Failed {
        details: Option<String>,
    },
    /// The agent session ended without usable output (typed at the ACP
    /// layer; never prose). Retrying may heal transients; persistent
    /// silence fails loudly instead of parsing hint text as JSON.
    NoOutput {
        details: Option<String>,
    },
}

impl AgentTaskError {
    pub fn details(&self) -> Option<&str> {
        match self {
            Self::NotConfigured { details }
            | Self::Failed { details }
            | Self::NoOutput { details } => details.as_deref(),
        }
    }
}

pub trait AgentInvoker: Send + Sync {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError>;
}
