//! Media inspection domain. Implementation lives in `ffprobe`.

pub mod error;
pub mod ffprobe;
pub mod model;
pub mod service;
pub mod tools;

pub use error::{MediaError, MediaErrorCode};
pub use model::{MediaInfo, MediaStream, StreamKind};
pub use service::MediaInspector;
