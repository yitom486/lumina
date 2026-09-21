//! ACP domain types: pure data, no IO, no child processes.

pub mod context;
pub mod environment;
pub mod model;
pub mod settings;

pub use context::VideoPromptContext;
pub use environment::{set_default_environment, SessionEnvironment};
pub use model::{
    AcpEvent, AcpModelDiscoveryResult, AcpSessionModelOptions, AcpSessionModelSelection,
    AcpSessionOption, AcpStatus, AgentExecutionRequest, AgentKind, AgentProfileInput,
    AgentProfileStatus, AgentProfilesHint, AgentSessionInfo, AgentSessionListResult, AuthPolicy,
    EnvPreset, LauncherPreset, PermissionOption, PromptImage, ResumeOutcome, SavedSessionHint,
    SessionKind, SessionStoragePreset,
};
pub use settings::{AcpClientSettings, PermissionMode, ThinkingLevel};
