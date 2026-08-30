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

### Playback control (M6)

- Commands：`open` / `play` / `pause` / `stop` / `seek` / `set_volume` / `set_rate`
- `player_subscribe(onEvent)`：前端传入 Tauri `Channel<PlayerEvent>`
- 后台 ticker ~200ms 推送 `PositionChanged`（禁止 React 10ms invoke 轮询）
- 事件：`StateChanged` / `PositionChanged` / `DurationChanged` / `FileLoaded` / `Ended` / `Error`
- 换文件：同一 mpv 实例上 stop → loadfile，不创建第二后端

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

### Frontend (M7)

```
src/
  App.tsx                 # 组装壳，无业务细节
  layouts/AppShell.tsx    # 标题栏 + 主列
  features/player/        # 播放器特性模块
    types.ts / api.ts / store.ts
    hooks/                # Channel 订阅、surface bounds
    components/           # VideoSurface、PlayerBar、控件
  components/ui/          # shadcn
  lib/                    # format 等纯工具
```

Zustand 只镜像 Rust；actions 一律走 Tauri Command；时间轴走 Channel。

### Hardening (M8)

- open 前校验路径（空 / 不存在 / 非文件 / 空文件）→ `LoadError` / `UnsupportedMedia`
- `loadfile` 失败按文案分类为 Load / Unsupported
- 退出：`mark_shutdown` → drop Channel → `PlayerService::shutdown`（drop mpv）→ `VideoSurface::Drop`（DestroyWindow）
- 业务代码无 `unwrap`/`expect`；`pnpm lint` = `tsc --noEmit`

### Media inspection (Phase 2)

```
media_inspect(path) → MediaInspector → ffprobe (native/ffmpeg/) → MediaInfo
```

- 与 Player / libmpv 解耦；只读探测，不转码
- 前端 `features/media` + TanStack Query；打开文件后底部面板展示

### Subtitle / Transcript (Phase 3)

```
subtitle_list_tracks / subtitle_load_transcript
  → SubtitleService → ffmpeg 抽出 → SRT/ASS/VTT 解析 → Transcript
```

- 字幕下拉：内嵌轨 + 同名/同 stem 外挂（`video.srt`、`video.en.srt`）自动发现
- 用户只做选择题；位图轨可上画面，文稿需文本或按需 ASR
- 文稿：当前句高亮、点击 seek

### ASR (Phase 4, on-demand)

```
asr_transcribe(path)  // only when user clicks
  → ffmpeg wav 16k mono
  → whisper-cli (native/whisper/, optional)
  → Transcript
```

- 启动 / Open **不**加载模型
- 未放置 whisper-cli 时返回 `NotConfigured`，不影响播放

