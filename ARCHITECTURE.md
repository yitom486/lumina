# ARCHITECTURE

## Current (Phase 1)

```
React
  → Tauri Command / Channel
    → Rust Application Layer
      → PlayerService
        → LibMpvPlayer
          → libmpv
```

React 不持有 mpv handle、native window handle、FFI pointer。

Domain API（`src-tauri/src/player/`）与 libmpv 实现（`src-tauri/src/player/mpv/`）分离。

Player Runtime（`AppState` → `Mutex<PlayerService>` + `VideoSurface`）由 Tauri State 管理。

### Domain (M3)

- `PlayerState`: Idle | Loading | Ready | Playing | Paused | Ended | Error
- `PlayerError`: `{ code, message, details? }`
- `PlayerService` + Commands（含 `player_set_surface_bounds`）

### libmpv (M4)

- Crate：`libmpv2` 6.x；本地 `src-tauri/native/mpv/`
- `build.rs` 链接并放置 `libmpv-2.dll`

### Native surface (M5)

**方案（Windows）：子 HWND + `wid`**

1. 从 Tauri `WebviewWindow` 取父 `HWND`（`HasWindowHandle` / Win32）
2. 创建 `WS_CHILD | WS_CLIPSIBLINGS` 子窗口 `LuminaMpvSurface`
3. 仅覆盖前端量到的视频矩形（`player_set_surface_bounds`，含 DPI scale）
4. `SetWindowPos(HWND_TOP)`，让视频子窗口盖在 WebView 之上；底部 HTML 控件区域不被遮挡
5. `LibMpvPlayer::initialize_with_wid(hwnd)`；`hwdec=auto`（失败则软件解码）
6. `player_open` → `loadfile` 真实本地文件

不使用 HTML `<video>` / canvas 逐帧拷贝。macOS / Linux 表面在 Phase 1 未实现。
