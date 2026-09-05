# ARCHITECTURE

## Current (0.3.0 — player chain unchanged since Phase 1; media/ACP phases in ROADMAP)

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
- `build.rs` 链接 libmpv；Windows delay-load `libmpv-2.dll`；Unix 优先 `native/mpv/runtime/` 或 pkg-config
- 安装包 resources：`native/mpv/runtime/` → `mpv/`（`tauri.conf.json`）

### Native surface (M5) — platform matrix (H-P1-4)

**方案：平台子 surface + libmpv `wid`（禁止 HTML `<video>`）**

| 平台 | 父 handle | 子 surface | 点击/双击 | Wayland/特殊行为 |
|------|-----------|------------|-----------|------------------|
| Windows | WebView 父 `HWND` | `LuminaMpvSurface` 子 HWND | 有（→ `SurfaceClick/DoubleClick`） | 主力冒烟平台 |
| macOS | AppKit `NSView` | 子 `NSView` | 无（已知 gap，未实现） | Y 轴翻转对齐 WebView |
| Linux | X11 父 window | `XCreateSimpleWindow` 子窗口 | 无（已知 gap，未实现） | Wayland 句柄 → `NativeWindowError` 明错并 fail-fast（`window/linux.rs`），绝不静默黑屏 |

1. setup：`ensure_libmpv_loaded` → `parent_handle_from_webview` → `VideoSurface::create` → `attach_backend_with_wid`
2. 前端 `player_set_surface_bounds` 量视频矩形（含 DPI scale）；子 surface 仅覆盖该区域
3. Windows：`SetWindowPos(HWND_TOP)` 盖在 WebView 之上，底部 HTML 控件不被遮挡
4. `LibMpvPlayer::initialize_with_wid`；`hwdec=auto`（失败则软件解码）
5. `player_open` → `loadfile` 真实本地文件

### Codec / subtitle matrix (H-P1-4)

| 轴 | 自动化覆盖 | 手工冒烟 |
|----|-----------|----------|
| H.264 / HEVC / AV1 探测 | `media::ffprobe::codec_matrix_probes_h264_hevc_av1`（现生成 64px fixture → `MediaInspector`；无 ffmpeg 的机器 SKIP） | 播放各 1 个真实文件（见下表） |
| SRT / ASS / VTT 解析 | `subtitle::parse` 单测（srt/ass/vtt + 空内容中文错） | 文稿高亮 + 点击 seek |
| 位图字幕 | `subtitle::extract` 编解码检测单测（上画面，不做文稿） | 上画面确认 |

播放本身不分编解码：三者共用同一 `loadfile` 路径，mpv 负责解码。

### Playback baseline (H-P1-5, half of Phase 8.0)

headless `vo=null` 真解码基线，唯一用途：日后 `CompositedGpu` 与 `WindowedHwnd`
同条件对比。只看同机同媒体，跨机器数字无意义。

```powershell
cd src-tauri
# 默认现生成 1080p60/10s（含音频）；自带文件则设 LUMINA_BASELINE_MEDIA
cargo test --lib baseline_local_playback -- --ignored --nocapture
$env:LUMINA_BASELINE_MEDIA = "D:\clips\movie4k60.mp4"
$env:LUMINA_BASELINE_OUT = "$env:TEMP\lumina-baseline.json"
cargo test --lib baseline_local_playback -- --ignored --nocapture
```

报告字段：媒体（分辨率/编码/时长）+ `hwdec` 上下文 + demux 就绪 + 首帧
（`time-pos>0`）+ 稳态推进率 + 三点 seek 延迟 + 解码/vo 丢帧 + avsync。

CPU/GPU 留手工：播放同一媒体同一窗口状态，任务管理器（Win）/
活动监视器（mac）记录进程 CPU 与 GPU 引擎占用，和 JSON 报告放一起。
跨平台 OS 计数器代码刻意不写（见 H-P1-5 L2）。

2026-09-06 本机基线（Win x64，生成 1080p60 H.264/10s）：hwdec `d3d11va-copy`，
demux 368ms，首帧 884ms，稳态 5000/5000ms，三点 seek 6/53/1ms，丢帧 0，avsync 0。

### Manual smoke checklist（待手工 — headless 未跑，不伪造结果）

- [ ] Windows：H264 / HEVC / AV1 各打开 1 个 → 首帧 + 10s 播放 + seek 2 次
- [ ] Windows：SRT / ASS / VTT 文稿 + 点击 seek；位图轨上画面
- [ ] Windows：视频面单击暂停/双击全屏
- [ ] macOS：NSView 打开 + seek；记录点击缺失影响
- [ ] Linux X11：打开 + seek；Wayland 会话启动即得中文明错（非黑屏）
- [ ] 缺 ffprobe 安装包：媒体信息区中文降级 + hint（`MediaInfoPanel.test.tsx` 已锁 UI）

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
- 业务代码无 `unwrap`/`expect`；`bun run lint` = `tsc --noEmit`

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

### ACP Chat (Phase 5)

```
App (parallel)
  ├─ playback column + sidebar (列表/文稿/笔记/章节)
  └─ ChatDock (fixed overlay, long-lived)
       └─ AcpPanel → Tauri acp_* → AcpService → Agent stdio
```

- 入口：标题栏 / 全屏浮层 **★ + 聊天** 按钮（`ChatToggleButton`）
- **不在 sidebar tab 内**；首次打开后 `chatMounted` 直至退出应用
- `chatOpen` 仅控制显隐；全屏、切 sidebar tab 不 unmount
- 按需 `acp_connect` / `acp_prompt`；禁止启动预连

