//! Agent invocation port for AI tasks (translation, polishing).
//!
//! AI crates never implement ACP or model APIs. The app provides the
//! ACP-backed implementation; tasks stay short-lived and data-isolated.
//! Home is `lumina-core` so every domain shares one port (M5 follow-up).

/// One isolated, data-only Agent call. No chat history, no MCP tools.
#[derive(Debug, Clone)]
pub struct IsolatedAgentTask {
    pub prompt: String,
    /// Complete prompt that can bootstrap a replacement session before an
    /// identical transport retry. Callers may use a smaller `prompt` after a
    /// slot has learned the job context, but recovery must remain standalone.
    pub bootstrap_prompt: Option<String>,
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

/// A job-scoped conversation used by batch pipelines.
///
/// The concrete implementation owns the session/slot lease. AI use-cases may
/// send several prompts through it (initial request, validation correction and
/// bounded retry) without knowing anything about ACP or process lifecycle.
pub trait AgentConversation: Send {
    fn prompt(&mut self, task: IsolatedAgentTask) -> Result<String, AgentTaskError>;

    /// Whether the next prompt should carry the full job context. A pooled
    /// slot returns false after its first successful prompt; one-shot
    /// fallbacks return true for every call.
    fn needs_bootstrap(&self) -> bool {
        true
    }
}

/// Fallback conversation for invokers that only expose one-shot calls. This
/// keeps existing test doubles and non-workshop callers source-compatible;
/// production subtitle pools override `open_conversation` with a real
/// slot-pinned implementation.
struct OneShotConversation<'a, T: AgentInvoker + ?Sized> {
    invoker: &'a T,
}

impl<T: AgentInvoker + ?Sized> AgentConversation for OneShotConversation<'_, T> {
    fn prompt(&mut self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        self.invoker.invoke_isolated(task)
    }
}

pub trait AgentInvoker: Send + Sync {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError>;

    /// Open a conversation for one logical batch. The default preserves the
    /// old one-shot behavior, while a workshop adapter returns a slot-pinned
    /// conversation whose session survives across batches.
    fn open_conversation<'a>(
        &'a self,
        _task_label: Option<String>,
    ) -> Result<Box<dyn AgentConversation + 'a>, AgentTaskError> {
        Ok(Box::new(OneShotConversation { invoker: self }))
    }
}
