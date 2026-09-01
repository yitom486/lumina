//! Native video surface for embedding libmpv (`wid`).

#[cfg(windows)]
mod win32;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(windows)]
pub use win32::{parent_handle_from_webview, register_surface_app, VideoSurface};

#[cfg(target_os = "macos")]
pub use macos::{parent_handle_from_webview, register_surface_app, VideoSurface};

#[cfg(target_os = "linux")]
pub use linux::{
    parent_x11_from_webview as parent_handle_from_webview, register_surface_app, VideoSurface,
};
