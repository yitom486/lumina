//! Optional on-demand Codex ACP. Not required for playback.

pub mod error;
pub mod model;
pub mod paths;
pub mod protocol;
pub mod service;

pub use error::{AcpError, AcpErrorCode};
pub use model::{AcpEvent, AcpStatus};
pub use service::AcpService;
