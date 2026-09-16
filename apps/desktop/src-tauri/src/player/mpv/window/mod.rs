//! Native video surface for embedding libmpv (`wid`).

// Linux/macOS currently keep their native surface event adapters unchanged;
// allow the shared contract to compile cleanly until those adapters consume it.
#[allow(dead_code)]
mod input;

use std::sync::OnceLock;

use tauri::{AppHandle, Manager};

use crate::player::model::PlayerEvent;
use crate::state::AppState;

static SURFACE_APP: OnceLock<AppHandle> = OnceLock::new();

/// Register the app handle used by every native surface adapter.
pub fn register_surface_app(app: AppHandle) {
    if SURFACE_APP.set(app).is_err() {
        tracing::debug!("surface app handle already registered");
    }
}

/// Forward one explicit surface event to the custom OSC script. Native
/// adapters call this shared path so their protocol and error handling stay
/// identical across platforms.
pub(crate) fn forward_surface_event(phase: &str, x: i32, y: i32) {
    let Some(app) = SURFACE_APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let result = state.with_player(|player| {
        player.forward_surface_event(phase, x, y);
        Ok(())
    });
    if let Err(error) = result {
        tracing::debug!(%error, phase, "forward surface event failed");
    }
}

/// Emit a native surface gesture after the platform adapter has completed its
/// click arbitration.
pub(crate) fn emit_surface_event(event: PlayerEvent) {
    let Some(app) = SURFACE_APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    state.emit(event);
}

/// Toggle playback after native click arbitration. The native surface may
/// receive a click before the frontend Channel is subscribed, so this stays
/// on the authoritative PlayerService path.
pub(crate) fn toggle_surface_play_pause() {
    let Some(app) = SURFACE_APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    match state.with_player(|player| player.toggle_play_pause()) {
        Ok((_, events)) => state.emit_all(events),
        Err(error) => tracing::debug!(%error, "surface click play/pause failed"),
    }
}

#[cfg(windows)]
mod win32;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(windows)]
pub use win32::{parent_handle_from_webview, VideoSurface};

#[cfg(target_os = "macos")]
pub use macos::{parent_handle_from_webview, VideoSurface};

#[cfg(target_os = "linux")]
pub use linux::{parent_x11_from_webview as parent_handle_from_webview, VideoSurface};
