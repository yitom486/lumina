//! Windows child HWND used as libmpv `wid` target.
//!
//! Sibling of WebView2, placed over the video rect only so HTML controls below
//! stay clickable. HWND is stored as `isize` so AppState stays `Send`.
//! Mouse clicks are forwarded as PlayerEvent (HTML cannot receive them under HWND).

use std::mem::size_of;
use std::sync::{Mutex, OnceLock};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::WebviewWindow;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetDoubleClickTime, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, KillTimer, LoadCursorW,
    MoveWindow, RegisterClassW, SetTimer, SetWindowPos, ShowWindow, CS_DBLCLKS, CS_HREDRAW,
    CS_OWNDC, CS_VREDRAW, HWND_TOP, IDC_ARROW, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    WM_CANCELMODE, WM_CAPTURECHANGED, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_CHILD,
    WS_CLIPSIBLINGS,
};

use super::{
    emit_surface_event, forward_surface_event, input::should_emit_surface_click,
    input::should_seek_immediately, input::MouseArbiter, input::PendingClick,
    toggle_surface_play_pause,
};
use crate::player::error::{PlayerError, PlayerErrorCode};
use crate::player::model::PlayerEvent;

static CLASS_REGISTERED: OnceLock<()> = OnceLock::new();
static INPUT_STATE: OnceLock<Mutex<MouseArbiter>> = OnceLock::new();
const CLICK_TIMER_ID: usize = 1;
const WM_MOUSELEAVE: u32 = 0x02A3;
fn input_state() -> &'static Mutex<MouseArbiter> {
    INPUT_STATE.get_or_init(|| Mutex::new(MouseArbiter::default()))
}

/// Decode client-area mouse position from `LPARAM` (signed 16-bit pairs).
fn surface_mouse_coords(lparam: LPARAM) -> (i32, i32) {
    let bits = lparam.0 as u32;
    let x = (bits & 0xFFFF) as u16 as i16 as i32;
    let y = ((bits >> 16) & 0xFFFF) as u16 as i16 as i32;
    (x, y)
}

/// Demo OSC: decode `WM_MOUSEWHEEL` delta (`GET_WHEEL_DELTA_WPARAM` sign).
fn wheel_delta(wparam: WPARAM) -> i32 {
    ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32
}

/// Demo OSC: wheel direction, `GET_WHEEL_DELTA_WPARAM` sign (> 0 is UP).
fn wheel_is_up(wparam: WPARAM) -> bool {
    wheel_delta(wparam) > 0
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
        WM_MOUSEMOVE => {
            let (x, y) = surface_mouse_coords(lparam);
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut track);
            let dragging = input_state()
                .lock()
                .map(|mut input| input.move_to(x, y))
                .unwrap_or(false);
            forward_surface_event(if dragging { "drag" } else { "move" }, x, y);
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let (x, y) = surface_mouse_coords(lparam);
            let (previous_click, accept_down) = input_state()
                .lock()
                .ok()
                .map(|mut input| input.left_press(x, y))
                .unwrap_or((None, false));
            if let Some(click) = previous_click {
                let _ = KillTimer(Some(hwnd), CLICK_TIMER_ID);
                dispatch_single_click(hwnd, click);
            }
            if accept_down {
                unsafe {
                    SetCapture(hwnd);
                }
                forward_surface_event("down", x, y);
            }
            return LRESULT(0);
        }
        WM_RBUTTONDOWN => {
            let (x, y) = surface_mouse_coords(lparam);
            let should_forward = input_state()
                .lock()
                .map(|mut input| input.right_press(x, y))
                .unwrap_or(false);
            if should_forward {
                forward_surface_event("right-down", x, y);
            }
            return LRESULT(0);
        }
        WM_RBUTTONUP => {
            let (x, y) = surface_mouse_coords(lparam);
            let should_forward = input_state()
                .lock()
                .map(|mut input| input.right_release(x, y))
                .unwrap_or(false);
            if should_forward {
                forward_surface_event("right-up", x, y);
            }
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            let (x, y) = input_state()
                .lock()
                .map(|input| input.last_pos)
                .unwrap_or((0, 0));
            forward_surface_event(
                if wheel_is_up(wparam) {
                    "wheel-up"
                } else {
                    "wheel-down"
                },
                x,
                y,
            );
            return LRESULT(0);
        }
        WM_LBUTTONDBLCLK => {
            let (x, y) = surface_mouse_coords(lparam);
            let had_down = input_state()
                .lock()
                .map(|mut input| input.double_click(x, y))
                .unwrap_or(false);
            let _ = KillTimer(Some(hwnd), CLICK_TIMER_ID);
            if had_down {
                // Safety fallback for a platform message sequence that
                // delivered the double-click before the first release.
                forward_surface_event("up", x, y);
            }
            forward_surface_event("double", x, y);
            if surface_click_zone(hwnd, y) {
                emit_surface_event(PlayerEvent::SurfaceDoubleClick);
            }
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let (x, y) = surface_mouse_coords(lparam);
            let (send_up, pending_click) = input_state()
                .lock()
                .map(|mut input| input.left_release(x, y))
                .unwrap_or((false, None));
            if send_up {
                forward_surface_event("up", x, y);
                let _ = ReleaseCapture();
            }
            if let Some(click) = pending_click {
                let mut rect = RECT::default();
                let (width, height) = if unsafe { GetClientRect(hwnd, &mut rect).is_ok() } {
                    (rect.right - rect.left, rect.bottom - rect.top)
                } else {
                    (0, 0)
                };
                if should_seek_immediately(click.x, click.y, width, height) {
                    // The seek bar is not a click/double-click gesture. Clear
                    // the pending entry and let Lua seek on this mouse-up.
                    let _ = input_state()
                        .lock()
                        .ok()
                        .and_then(|mut input| input.take_timer_click());
                    forward_surface_event("seek-click", click.x, click.y);
                    return LRESULT(0);
                }
                if !should_emit_surface_click(click.y, height) {
                    // OSC controls are ordinary buttons, not video gestures.
                    // Dispatch them on mouse-up instead of waiting for the
                    // video double-click disambiguation timer.
                    let _ = input_state()
                        .lock()
                        .ok()
                        .and_then(|mut input| input.take_timer_click());
                    dispatch_single_click(hwnd, click);
                    return LRESULT(0);
                }
                let timer_id = SetTimer(
                    Some(hwnd),
                    CLICK_TIMER_ID,
                    unsafe { GetDoubleClickTime().max(1) },
                    None,
                );
                if timer_id == 0 {
                    if let Some(click) = input_state()
                        .lock()
                        .ok()
                        .and_then(|mut input| input.take_timer_click())
                    {
                        dispatch_single_click(hwnd, click);
                    }
                }
            }
            return LRESULT(0);
        }
        WM_TIMER if wparam.0 == CLICK_TIMER_ID => {
            let _ = KillTimer(Some(hwnd), CLICK_TIMER_ID);
            if let Some(click) = input_state()
                .lock()
                .ok()
                .and_then(|mut input| input.take_timer_click())
            {
                dispatch_single_click(hwnd, click);
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            let dragging = input_state()
                .lock()
                .map(|input| input.left_down)
                .unwrap_or(false);
            if !dragging {
                let (x, y) = input_state()
                    .lock()
                    .map(|input| input.last_pos)
                    .unwrap_or((0, 0));
                forward_surface_event("leave", x, y);
            }
            return LRESULT(0);
        }
        WM_CAPTURECHANGED => {
            let (had_left_down, (x, y)) = input_state()
                .lock()
                .map(|mut input| {
                    let pos = input.last_pos;
                    // ReleaseCapture after an ordinary mouse-up also emits
                    // WM_CAPTURECHANGED. The click has already been moved to
                    // pending_click, so do not cancel that pending single
                    // click unless a button is still held.
                    (input.capture_changed(), pos)
                })
                .unwrap_or((false, (0, 0)));
            if had_left_down {
                forward_surface_event("cancel", x, y);
            }
            return LRESULT(0);
        }
        WM_CANCELMODE | WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), CLICK_TIMER_ID);
            let (had_left_down, (x, y)) = input_state()
                .lock()
                .map(|mut input| {
                    let pos = input.last_pos;
                    (input.cancel(), pos)
                })
                .unwrap_or((false, (0, 0)));
            if had_left_down {
                forward_surface_event("cancel", x, y);
            }
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn surface_click_zone(hwnd: HWND, y: i32) -> bool {
    let mut rect = RECT::default();
    let height = if unsafe { GetClientRect(hwnd, &mut rect).is_ok() } {
        rect.bottom - rect.top
    } else {
        0
    };
    should_emit_surface_click(y, height)
}

fn dispatch_single_click(hwnd: HWND, click: PendingClick) {
    forward_surface_event("click", click.x, click.y);
    if surface_click_zone(hwnd, click.y) {
        toggle_surface_play_pause();
    }
}

fn native_error(message: &str, details: Option<String>) -> PlayerError {
    PlayerError::new(PlayerErrorCode::NativeWindowError, message, details)
}
