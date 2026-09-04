//! Optional on-demand yt-dlp (online source resolver). Never loaded at startup.

pub mod download;
pub mod error;
pub mod model;
pub mod paths;
pub mod resolve;
pub mod service;

pub use error::{YtdlError, YtdlErrorCode};
pub use model::{YtdlFormat, YtdlInstallEvent, YtdlResolveResult, YtdlStatus, YtdlSubtitleTrack};
pub use service::YtdlService;
