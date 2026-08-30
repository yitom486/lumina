//! Native video surface for embedding libmpv (`wid`).

#[cfg(windows)]
mod win32;

#[cfg(windows)]
pub use win32::{hwnd_from_webview_window, register_surface_app, VideoSurface};

#[cfg(not(windows))]
mod unsupported {
    use crate::player::error::{PlayerError, PlayerErrorCode};
    use tauri::WebviewWindow;

    pub struct VideoSurface;

    impl VideoSurface {
        pub fn create(_parent_hwnd: isize) -> Result<Self, PlayerError> {
            Err(PlayerError::new(
                PlayerErrorCode::NativeWindowError,
                "native video surface is only implemented on Windows in Phase 1",
                None,
            ))
        }

        pub fn hwnd_i64(&self) -> i64 {
            0
        }

        pub fn set_bounds(
            &self,
            _x: i32,
            _y: i32,
            _width: i32,
            _height: i32,
        ) -> Result<(), PlayerError> {
            Err(PlayerError::new(
                PlayerErrorCode::NativeWindowError,
                "native video surface is only implemented on Windows in Phase 1",
                None,
            ))
        }
    }

    pub fn register_surface_app(_app: tauri::AppHandle) {}

    pub fn hwnd_from_webview_window(_window: &WebviewWindow) -> Result<isize, PlayerError> {
        Err(PlayerError::new(
            PlayerErrorCode::NativeWindowError,
            "native video surface is only implemented on Windows in Phase 1",
            None,
        ))
    }
}

#[cfg(not(windows))]
pub use unsupported::{hwnd_from_webview_window, register_surface_app, VideoSurface};
