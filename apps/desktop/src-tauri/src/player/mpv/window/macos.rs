//! macOS NSView child surface for libmpv `wid`.

// cocoa 已标 deprecated（建议迁 objc2）；当前 surface 路径仍依赖它。
// objc 宏会展开 `cfg(feature = "cargo-clippy")`，由 Cargo.toml check-cfg 声明。
#![allow(deprecated)]

use std::sync::OnceLock;

use cocoa::appkit::NSView;
use cocoa::base::{id, nil, NO, YES};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
#[allow(unused_imports)] // sel / sel_impl 供 msg_send! 宏展开使用
use objc::{class, msg_send, sel, sel_impl};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{AppHandle, WebviewWindow};

use crate::player::error::{PlayerError, PlayerErrorCode};

static SURFACE_APP: OnceLock<AppHandle> = OnceLock::new();

pub fn register_surface_app(app: AppHandle) {
    let _ = SURFACE_APP.set(app);
}

pub struct VideoSurface {
    view: isize,
}

impl VideoSurface {
    pub fn create(parent_handle: isize) -> Result<Self, PlayerError> {
        unsafe {
            let parent: id = parent_handle as id;
            if parent == nil {
                return Err(native_error(
                    "父视图无效",
                    Some("AppKit parent NSView is null".into()),
                ));
            }

            let child: id = msg_send![class!(NSView), alloc];
            let child: id = msg_send![child, initWithFrame: NSView::frame(parent)];
            let _: () = msg_send![parent, addSubview: child];
            let _: () = msg_send![child, setHidden: YES];

            tracing::info!(view = child as isize, "video surface NSView created");
            Ok(Self {
                view: child as isize,
            })
        }
    }

    pub fn wid_i64(&self) -> i64 {
        self.view as i64
    }

    pub fn set_bounds(&self, x: i32, y: i32, width: i32, height: i32) -> Result<(), PlayerError> {
        unsafe {
            let view: id = self.view as id;
            if width <= 0 || height <= 0 {
                let _: () = msg_send![view, setHidden: YES];
                return Ok(());
            }

            let parent: id = msg_send![view, superview];
            let parent_frame = NSView::frame(parent);
            let y_flipped = parent_frame.size.height - (y as f64 + height as f64);
            let frame = NSRect::new(
                NSPoint::new(x as f64, y_flipped),
                NSSize::new(width as f64, height as f64),
            );
            let _: () = msg_send![view, setFrame: frame];
            let _: () = msg_send![view, setHidden: NO];
        }
        Ok(())
    }
}

impl Drop for VideoSurface {
    fn drop(&mut self) {
        if self.view != 0 {
            unsafe {
                let view: id = self.view as id;
                let _: () = msg_send![view, removeFromSuperview];
            }
            self.view = 0;
        }
    }
}

unsafe impl Send for VideoSurface {}
unsafe impl Sync for VideoSurface {}

pub fn parent_handle_from_webview(window: &WebviewWindow) -> Result<isize, PlayerError> {
    let handle = window
        .window_handle()
        .map_err(|error| native_error("无法获取窗口句柄", Some(error.to_string())))?;

    match handle.as_raw() {
        RawWindowHandle::AppKit(appkit) => Ok(appkit.ns_view.as_ptr() as isize),
        other => Err(native_error(
            "窗口句柄类型不正确",
            Some(format!("expected AppKit, got {other:?}")),
        )),
    }
}

fn native_error(message: &str, details: Option<String>) -> PlayerError {
    PlayerError::new(PlayerErrorCode::NativeWindowError, message, details)
}
