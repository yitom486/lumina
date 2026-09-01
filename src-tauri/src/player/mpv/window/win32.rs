//! Windows child HWND used as libmpv `wid` target.
//!
//! Sibling of WebView2, placed over the video rect only so HTML controls below
//! stay clickable. HWND is stored as `isize` so AppState stays `Send`.
//! Mouse clicks are forwarded as PlayerEvent (HTML cannot receive them under HWND).

use std::sync::OnceLock;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{AppHandle, Manager, WebviewWindow};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, LoadCursorW, MoveWindow, RegisterClassW,
    SetWindowPos, ShowWindow, CS_DBLCLKS, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, HWND_TOP, IDC_ARROW,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WNDCLASSW,
    WS_CHILD, WS_CLIPSIBLINGS,
};

use crate::player::error::{PlayerError, PlayerErrorCode};
use crate::player::model::PlayerEvent;
use crate::state::AppState;

static CLASS_REGISTERED: OnceLock<()> = OnceLock::new();
static SURFACE_APP: OnceLock<AppHandle> = OnceLock::new();

/// Call once from setup so the surface WndProc can emit player events.
pub fn register_surface_app(app: AppHandle) {
    if SURFACE_APP.set(app).is_err() {
        tracing::debug!("surface app handle already registered");
    }
}

fn emit_surface_event(event: PlayerEvent) {
    let Some(app) = SURFACE_APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    state.emit(event);
}

pub struct VideoSurface {
    hwnd: isize,
}

impl VideoSurface {
    pub fn create(parent_hwnd: isize) -> Result<Self, PlayerError> {
        ensure_window_class()?;

        let parent = HWND(parent_hwnd as *mut _);
        if parent.0.is_null() {
            return Err(native_error(
                "父窗口无效",
                Some("parent HWND is null".into()),
            ));
        }

        let module = unsafe {
            GetModuleHandleW(None).map_err(|error| {
                native_error(
                    "无法获取模块句柄",
                    Some(format!("GetModuleHandleW: {error}")),
                )
            })?
        };

        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("LuminaMpvSurface"),
                w!("lumina-mpv-surface"),
                WS_CHILD | WS_CLIPSIBLINGS,
                0,
                0,
                1,
                1,
                Some(parent),
                None,
                Some(module.into()),
                None,
            )
        }
        .map_err(|error| {
            native_error(
                "无法创建视频窗口",
                Some(format!("CreateWindowExW: {error}")),
            )
        })?;

        if hwnd.0.is_null() {
            return Err(native_error(
                "无法创建视频窗口",
                Some("CreateWindowExW returned null HWND".into()),
            ));
        }

        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }

        let hwnd_value = hwnd.0 as isize;
        tracing::info!(hwnd = hwnd_value, "video surface HWND created");
        Ok(Self { hwnd: hwnd_value })
    }

    pub fn wid_i64(&self) -> i64 {
        self.hwnd as i64
    }

    /// Back-compat alias for `wid_i64`.
    pub fn hwnd_i64(&self) -> i64 {
        self.wid_i64()
    }

    fn as_hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    pub fn set_bounds(&self, x: i32, y: i32, width: i32, height: i32) -> Result<(), PlayerError> {
        let hwnd = self.as_hwnd();
        if width <= 0 || height <= 0 {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            return Ok(());
        }

        unsafe {
            MoveWindow(hwnd, x, y, width, height, true).map_err(|error| {
                native_error("无法调整视频窗口位置", Some(format!("MoveWindow: {error}")))
            })?;
            SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                x,
                y,
                width,
                height,
                SWP_SHOWWINDOW | SWP_NOACTIVATE,
            )
            .map_err(|error| {
                native_error(
                    "无法调整视频窗口层级",
                    Some(format!("SetWindowPos: {error}")),
                )
            })?;
        }

        Ok(())
    }
}

impl Drop for VideoSurface {
    fn drop(&mut self) {
        if self.hwnd != 0 {
            tracing::info!(hwnd = self.hwnd, "destroying video surface HWND");
            unsafe {
                let _ = DestroyWindow(self.as_hwnd());
            }
            self.hwnd = 0;
        }
    }
}

// SAFETY: HWND values are process-local integers; Win32 allows moving them across
// threads for DestroyWindow/MoveWindow as long as creation stays on the UI thread.
unsafe impl Send for VideoSurface {}
unsafe impl Sync for VideoSurface {}

pub fn parent_handle_from_webview(window: &WebviewWindow) -> Result<isize, PlayerError> {
    let handle = window
        .window_handle()
        .map_err(|error| native_error("无法获取窗口句柄", Some(error.to_string())))?;

    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Ok(win32.hwnd.get()),
        other => Err(native_error(
            "窗口句柄类型不正确",
            Some(format!("expected Win32, got {other:?}")),
        )),
    }
}

fn ensure_window_class() -> Result<(), PlayerError> {
    if CLASS_REGISTERED.get().is_some() {
        return Ok(());
    }

    let module = unsafe {
        GetModuleHandleW(None).map_err(|error| {
            native_error(
                "无法获取模块句柄",
                Some(format!("GetModuleHandleW: {error}")),
            )
        })?
    };

    let cursor = unsafe {
        LoadCursorW(None, IDC_ARROW).map_err(|error| {
            native_error("无法加载鼠标光标", Some(format!("LoadCursorW: {error}")))
        })?
    };

    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC | CS_DBLCLKS,
        lpfnWndProc: Some(surface_wnd_proc),
        hInstance: module.into(),
        hCursor: cursor,
        hbrBackground: HBRUSH(std::ptr::null_mut()),
        lpszClassName: w!("LuminaMpvSurface"),
        ..Default::default()
    };

    let atom = unsafe { RegisterClassW(&class) };
    if atom == 0 {
        tracing::debug!("RegisterClassW returned 0; assuming class already registered");
    }

    let _ = CLASS_REGISTERED.set(());
    Ok(())
}

unsafe extern "system" fn surface_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_LBUTTONDBLCLK => {
            emit_surface_event(PlayerEvent::SurfaceDoubleClick);
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            emit_surface_event(PlayerEvent::SurfaceClick);
            return LRESULT(0);
        }
        WM_DESTROY => return LRESULT(0),
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn native_error(message: &str, details: Option<String>) -> PlayerError {
    PlayerError::new(PlayerErrorCode::NativeWindowError, message, details)
}
