//! Windows child HWND used as libmpv `wid` target.
//!
//! Sibling of WebView2, placed over the video rect only so HTML controls below
//! stay clickable. HWND is stored as `isize` so AppState stays `Send`.
//! Mouse clicks are forwarded as PlayerEvent (HTML cannot receive them under HWND).

use std::mem::size_of;
use std::sync::{Mutex, OnceLock};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{AppHandle, Manager, WebviewWindow};
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

use crate::player::error::{PlayerError, PlayerErrorCode};
use crate::player::model::PlayerEvent;
use crate::state::AppState;

static CLASS_REGISTERED: OnceLock<()> = OnceLock::new();
static SURFACE_APP: OnceLock<AppHandle> = OnceLock::new();
static INPUT_STATE: OnceLock<Mutex<MouseArbiter>> = OnceLock::new();

/// The default video zone is the only place where a single/double click is
/// surfaced to React. OSC hit-testing remains inside the Lua script.
const OSC_BOTTOM_ZONE_PX: i32 = 140;
const OSC_MARGIN_PX: i32 = 16;
const OSC_BOTTOM_H_PX: i32 = 68;
const OSC_GAP_PX: i32 = 12;
const OSC_SEEK_H_PX: i32 = 10;
const OSC_SEEK_HIT_PAD_PX: i32 = 12;

/// Demo OSC topbar 占位估计（px）：顶部此高度内的点击归 mpv 所有（自写 Lua 皮肤顶部 SUB/AUD 按钮）。
/// 必须与 native/osc/lumina-osc.lua 样式头顶部 metrics 保持同步，改一边记得改另一边。
const TOP_ZONE_PX: i32 = 64;
const CLICK_TIMER_ID: usize = 1;
const DRAG_THRESHOLD_PX: i32 = 4;
const WM_MOUSELEAVE: u32 = 0x02A3;

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

/// Toggle playback in the domain service after native click arbitration. Do
/// not route this through React: the native child HWND can receive the click
/// before the Channel is subscribed, while PlayerService already owns the
/// authoritative snapshot and emits the resulting state event.
fn toggle_surface_play_pause() {
    let Some(app) = SURFACE_APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let result = state.with_player(|player| player.toggle_play_pause());
    match result {
        Ok((_, events)) => state.emit_all(events),
        Err(error) => tracing::debug!(%error, "surface click play/pause failed"),
    }
}

/// Demo OSC: whether a `WM_LBUTTONUP` at client y should emit `SurfaceClick`.
/// `height <= 0` fails open (emit), matching the `GetClientRect` failure fallback.
fn should_emit_surface_click(y: i32, height: i32) -> bool {
    if height <= 0 {
        return true;
    }
    if y < TOP_ZONE_PX {
        return false;
    }
    y < height - OSC_BOTTOM_ZONE_PX
}

/// The seek bar is an immediate control. It must not enter the single/double
/// click disambiguation timer: a click on the timeline is neither a play/pause
/// gesture nor a fullscreen gesture.
fn should_seek_immediately(x: i32, y: i32, width: i32, height: i32) -> bool {
    if width <= 2 * OSC_MARGIN_PX || height <= OSC_MARGIN_PX + OSC_BOTTOM_H_PX {
        return false;
    }
    let bar_x = OSC_MARGIN_PX;
    let bar_w = width - 2 * OSC_MARGIN_PX;
    let bar_y = height - OSC_MARGIN_PX - OSC_BOTTOM_H_PX;
    let seek_x = bar_x + OSC_GAP_PX;
    let seek_w = bar_w - 2 * OSC_GAP_PX;
    let seek_y = bar_y + OSC_GAP_PX;
    let hit_y = seek_y - OSC_SEEK_HIT_PAD_PX;
    let hit_h = OSC_SEEK_H_PX + 2 * OSC_SEEK_HIT_PAD_PX;
    seek_w > 0 && x >= seek_x && x <= seek_x + seek_w && y >= hit_y && y <= hit_y + hit_h
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingClick {
    x: i32,
    y: i32,
}

#[derive(Debug, Default)]
struct MouseArbiter {
    left_down: bool,
    right_down: bool,
    dragged: bool,
    suppress_left_up: bool,
    down_at: Option<(i32, i32)>,
    last_pos: (i32, i32),
    pending_click: Option<PendingClick>,
}

impl MouseArbiter {
    fn move_to(&mut self, x: i32, y: i32) -> bool {
        self.last_pos = (x, y);
        if let Some((down_x, down_y)) = self.down_at {
            let dx = i64::from(x) - i64::from(down_x);
            let dy = i64::from(y) - i64::from(down_y);
            if dx * dx + dy * dy >= i64::from(DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) {
                self.dragged = true;
            }
        }
        self.left_down && self.dragged
    }

    /// Starts a press and returns a previous pending click that must be
    /// delivered first when Windows reports a second ordinary click rather
    /// than `WM_LBUTTONDBLCLK`.
    fn left_press(&mut self, x: i32, y: i32) -> (Option<PendingClick>, bool) {
        let pending = self.pending_click.take();
        if self.left_down {
            return (pending, false);
        }
        self.left_down = true;
        self.dragged = false;
        self.suppress_left_up = false;
        self.down_at = Some((x, y));
        self.last_pos = (x, y);
        (pending, true)
    }

    fn left_release(&mut self, x: i32, y: i32) -> (bool, Option<PendingClick>) {
        self.last_pos = (x, y);
        if self.suppress_left_up {
            self.suppress_left_up = false;
            return (false, None);
        }
        if !self.left_down {
            return (false, None);
        }
        self.left_down = false;
        self.down_at = None;
        let dragged = self.dragged;
        self.dragged = false;
        if dragged {
            (true, None)
        } else {
            let click = PendingClick { x, y };
            self.pending_click = Some(click);
            (true, Some(click))
        }
    }

    /// Cancels a pending single click and marks the trailing up as belonging
    /// to the double click. The bool is only a safety fallback for a platform
    /// sequence that delivers the double-click message before the first up.
    fn double_click(&mut self, x: i32, y: i32) -> bool {
        let had_down = self.left_down;
        self.pending_click = None;
        self.left_down = false;
        self.down_at = None;
        self.dragged = false;
        self.suppress_left_up = true;
        self.last_pos = (x, y);
        had_down
    }

    fn take_timer_click(&mut self) -> Option<PendingClick> {
        self.pending_click.take()
    }

    fn right_press(&mut self, x: i32, y: i32) -> bool {
        self.last_pos = (x, y);
        if self.right_down {
            false
        } else {
            self.right_down = true;
            true
        }
    }

    fn right_release(&mut self, x: i32, y: i32) -> bool {
        self.last_pos = (x, y);
        if self.right_down {
            self.right_down = false;
            true
        } else {
            false
        }
    }

    fn cancel(&mut self) -> bool {
        let had_left_down = self.left_down;
        self.left_down = false;
        self.right_down = false;
        self.dragged = false;
        self.suppress_left_up = false;
        self.down_at = None;
        self.pending_click = None;
        had_left_down
    }

    fn capture_changed(&mut self) -> bool {
        let had_left_down = self.left_down;
        if had_left_down {
            self.cancel();
        }
        had_left_down
    }
}

/// Demo OSC: decode client-area mouse position from `LPARAM` (signed 16-bit pairs).
fn surface_mouse_coords(lparam: LPARAM) -> (i32, i32) {
    let bits = lparam.0 as u32;
    let x = (bits & 0xFFFF) as u16 as i16 as i32;
    let y = ((bits >> 16) & 0xFFFF) as u16 as i16 as i32;
    (x, y)
}

fn input_state() -> &'static Mutex<MouseArbiter> {
    INPUT_STATE.get_or_init(|| Mutex::new(MouseArbiter::default()))
}

/// Forward one explicit surface event to Lua. This is the only OSC input path.
fn forward_surface_event(phase: &str, x: i32, y: i32) {
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

#[cfg(test)]
mod tests {
    use super::{
        should_emit_surface_click, should_seek_immediately, MouseArbiter, PendingClick, TOP_ZONE_PX,
    };

    #[test]
    fn surface_click_bottom_zone_exemption() {
        let height = 600;
        let cases: [(i32, bool); 11] = [
            (height - 1, false),
            (height - 140, false),
            (height - 141, true),
            (height + 50, false),
            (-10, false),
            (0, false),
            (32, false),
            (TOP_ZONE_PX - 1, false),
            (TOP_ZONE_PX, true),
            (TOP_ZONE_PX + 1, true),
            (100, true),
        ];
        for (y, expected) in cases {
            assert_eq!(
                should_emit_surface_click(y, height),
                expected,
                "y={y} height={height}"
            );
        }
        assert!(should_emit_surface_click(0, 0), "height=0 fail-open");
        assert!(
            should_emit_surface_click(0, -100),
            "negative height fail-open"
        );
    }

    #[test]
    fn seek_bar_click_is_immediate_and_outside_surface_gesture_zone() {
        let width = 1000;
        let height = 700;
        assert!(should_seek_immediately(500, 628, width, height));
        assert!(!should_seek_immediately(500, 600, width, height));
        assert!(!should_seek_immediately(20, 628, width, height));
        assert!(should_emit_surface_click(500, height));
    }

    #[test]
    fn double_click_cancels_first_single_click_and_suppresses_trailing_up() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_press(10, 20), (None, true));
        assert_eq!(
            input.left_release(10, 20),
            (true, Some(PendingClick { x: 10, y: 20 }))
        );
        assert!(!input.double_click(10, 20));
        assert_eq!(input.take_timer_click(), None);
        assert_eq!(input.left_release(10, 20), (false, None));
    }

    #[test]
    fn capture_change_after_mouse_up_keeps_pending_single_click() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_press(10, 20), (None, true));
        assert_eq!(
            input.left_release(10, 20),
            (true, Some(PendingClick { x: 10, y: 20 }))
        );
        assert!(!input.capture_changed());
        assert_eq!(
            input.take_timer_click(),
            Some(PendingClick { x: 10, y: 20 })
        );
    }

    #[test]
    fn dragging_forwards_release_but_does_not_create_single_click() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_press(10, 20), (None, true));
        assert!(!input.move_to(12, 20));
        assert!(input.move_to(20, 20));
        assert_eq!(input.left_release(20, 20), (true, None));
        assert_eq!(input.take_timer_click(), None);
    }

    #[test]
    fn stray_left_up_is_ignored() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_release(10, 20), (false, None));
    }

    #[test]
    fn duplicate_left_down_does_not_create_an_unpaired_input_event() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_press(10, 20), (None, true));
        assert_eq!(input.left_press(10, 20), (None, false));
        assert!(input.left_release(10, 20).0);
    }
}
