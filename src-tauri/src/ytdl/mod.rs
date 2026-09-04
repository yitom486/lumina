//! Optional on-demand yt-dlp (online source resolver). Never loaded at startup.

pub mod cookies;
pub mod download;
pub mod error;
pub mod model;
pub mod paths;
pub mod playback;
pub mod resolve;
pub mod service;
pub mod subtitle;

pub use cookies::{
    BrowserProfileOption, CookieBrowser, CookieMode, YtdlCookieConfigInput, YtdlCookieStatus,
    YtdlCookieTestResult,
};
pub use error::{YtdlError, YtdlErrorCode};
pub use model::{YtdlFormat, YtdlInstallEvent, YtdlResolveResult, YtdlStatus, YtdlSubtitleTrack};
pub use playback::{PlaybackFormatOption, PlaybackFormatsResponse, YtdlPlayTarget};
pub use service::YtdlService;
