//! Linux X11 child window for libmpv `wid`.

use std::ffi::CString;
use std::ptr;
use std::sync::OnceLock;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use tauri::{AppHandle, WebviewWindow};
use x11::xlib::{XCreateSimpleWindow, XDestroyWindow, XMapWindow, XMoveResizeWindow, XUnmapWindow};

use crate::player::error::{PlayerError, PlayerErrorCode};

static SURFACE_APP: OnceLock<AppHandle> = OnceLock::new();

pub fn register_surface_app(app: AppHandle) {
    let _ = SURFACE_APP.set(app);
}

pub struct VideoSurface {
    display: isize,
    window: u64,
}

impl VideoSurface {
    pub fn create(parent: ParentX11) -> Result<Self, PlayerError> {
        unsafe {
            let display = parent.display as *mut x11::xlib::Display;
            if display.is_null() || parent.window == 0 {
                return Err(native_error(
                    "父窗口无效",
                    Some("X11 display or parent window is null".into()),
                ));
            }

            let window = XCreateSimpleWindow(display, parent.window, 0, 0, 1, 1, 0, 0, 0);
            if window == 0 {
                return Err(native_error(
                    "无法创建视频窗口",
                    Some("XCreateSimpleWindow returned 0".into()),
                ));
            }

            XUnmapWindow(display, window);
            tracing::info!(window, "video surface X11 window created");
            Ok(Self {
                display: parent.display,
                window,
            })
        }
    }

    pub fn wid_i64(&self) -> i64 {
        self.window as i64
    }

    pub fn set_bounds(&self, x: i32, y: i32, width: i32, height: i32) -> Result<(), PlayerError> {
        unsafe {
            let display = self.display as *mut x11::xlib::Display;
            if width <= 0 || height <= 0 {
                XUnmapWindow(display, self.window);
                return Ok(());
            }
            XMoveResizeWindow(display, self.window, x, y, width as u32, height as u32);
            XMapWindow(display, self.window);
        }
        Ok(())
    }
}

impl Drop for VideoSurface {
    fn drop(&mut self) {
        if self.window != 0 {
            unsafe {
                let display = self.display as *mut x11::xlib::Display;
                XDestroyWindow(display, self.window);
            }
            self.window = 0;
        }
    }
}

unsafe impl Send for VideoSurface {}
unsafe impl Sync for VideoSurface {}

pub struct ParentX11 {
    display: isize,
    window: u64,
}

pub fn parent_x11_from_webview(window: &WebviewWindow) -> Result<ParentX11, PlayerError> {
    let window_handle = window
        .window_handle()
        .map_err(|error| native_error("无法获取窗口句柄", Some(error.to_string())))?;
    let display_handle = window
        .display_handle()
        .map_err(|error| native_error("无法获取显示句柄", Some(error.to_string())))?;

    match (window_handle.as_raw(), display_handle.as_raw()) {
        (RawWindowHandle::Xlib(xlib), RawDisplayHandle::Xlib(display)) => {
            let display_ptr = display
                .display
                .map(|ptr| ptr.as_ptr() as isize)
                .unwrap_or(0);
            if display_ptr == 0 {
                return Err(native_error(
                    "父窗口无效",
                    Some("Xlib display pointer is null".into()),
                ));
            }
            Ok(ParentX11 {
                display: display_ptr,
                window: xlib.window,
            })
        }
        (_, RawDisplayHandle::Wayland(_)) | (RawWindowHandle::Wayland(_), _) => Err(native_error(
            "当前 Linux 会话为 Wayland，暂不支持原生视频窗口",
            Some("use an X11 session or XWayland for native playback".into()),
        )),
        (other_window, other_display) => Err(native_error(
            "窗口句柄类型不正确",
            Some(format!(
                "expected Xlib window+display, got window={other_window:?} display={other_display:?}"
            )),
        )),
    }
}

fn native_error(message: &str, details: Option<String>) -> PlayerError {
    PlayerError::new(PlayerErrorCode::NativeWindowError, message, details)
}
