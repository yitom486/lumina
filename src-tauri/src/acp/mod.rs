//! Optional on-demand ACP. Not required for playback.

pub mod context;
pub mod discover;
pub mod error;
pub mod host;
pub mod model;
pub mod paths;
pub mod profile;
pub mod protocol;
pub mod settings;
pub mod service;

pub use context::VideoPromptContext;
pub use error::{AcpError, AcpErrorCode};
pub use model::{AcpEvent, AcpStatus, AgentProfileInput, PermissionOption, SavedSessionHint};
pub use profile::{AgentKind, AgentProfile, AgentProfileStatus};
pub use service::AcpService;
pub use settings::{AcpClientSettings, PermissionMode, ThinkingLevel};
