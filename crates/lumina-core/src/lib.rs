//! Stable value objects shared across domains.
//!
//! No Tauri, libmpv, file-library, ACP, MCP, or engine code lives here.
//! User-facing messages stay owned by each domain's error constructors;
//! [`MediaSourceError::message`] only exposes the stable strings so existing
//! mappings (player `LoadError`/`UnsupportedMedia`, ytdl `InvalidRequest`)
//! keep producing byte-identical errors.

pub mod agent_invoker;
pub mod checkpoint;
pub mod media_source;
pub mod tool_contract;

pub use agent_invoker::{AgentInvoker, AgentTaskError, IsolatedAgentTask};
pub use checkpoint::{BatchCheckpoint, CheckpointBatch};

pub use media_source::{MediaSource, MediaSourceError, MediaSourceErrorKind, MediaSourceKind};
