//! Optional on-demand ACP Client (library crate). Not required for playback.

pub mod agent_reply_collector;
pub mod context;
pub mod discover;
pub mod environment;
pub mod error;
pub mod host;
pub mod model;
pub mod paths;
pub mod profile;
pub mod protocol;
pub mod service;
pub mod settings;
pub mod workshop;

mod process;

pub use context::VideoPromptContext;
pub use environment::{set_default_environment, SessionEnvironment};
pub use error::{AcpError, AcpErrorCode};
pub use model::{
    AcpEvent, AcpModelDiscoveryResult, AcpSessionModelOptions, AcpSessionModelSelection,
    AcpSessionOption, AcpStatus, AgentProfileInput, AgentProfilesHint, PermissionOption,
    SavedSessionHint,
};
pub use profile::{AgentKind, AgentProfile, AgentProfileStatus};
pub use service::AcpService;
pub use settings::{AcpClientSettings, PermissionMode, ThinkingLevel};
pub use workshop::{PoolConfig, WorkshopPool, DEFAULT_POOL_SIZE};
