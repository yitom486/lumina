//! Media inspection domain. Implementation lives in `ffprobe`.

pub mod error;
pub mod ffprobe;
pub mod model;
pub mod service;
pub mod siblings;
pub mod tools;

pub use error::{MediaError, MediaErrorCode};
pub use model::{MediaChapter, MediaInfo, MediaStream, StreamKind};
pub use service::MediaInspector;
pub use siblings::list_sibling_videos;
