# Lumina — Agent Guide

AI Video Reader 桌面应用。当前只做 **Phase 1：Native Playback Spike**。

完整计划见 `project.md`。执行计划与状态在 `.plan/`（本地，不入库）。每完成一个可检查点，必须更新对应文档的 `status` 与 `STATUS.md`。实现前先读本文件与 `.cursor/rules/`。

## Tech Stack

- Desktop: **Tauri 2**
- Backend: **Rust**
- Frontend: **React + TypeScript + Vite**
- Package manager: **pnpm**
- UI: **Tailwind CSS + shadcn/ui**
- Client state: **Zustand**
- Async/server state: **TanStack Query**
- Player: **libmpv**（Rust 优先调查并使用维护中的 `libmpv2`）
- Window: Tauri Window + `HasWindowHandle` / `raw-window-handle`（勿用已废弃 API）

禁止用 HTML `<video>`、canvas 逐帧复制等方式伪装 native playback。

第一阶段只需当前开发平台跑通。Player 抽象不得绑定具体 OS，为 Windows / macOS / Linux 留边界。

## Architecture

```
React → Tauri Command / Channel → PlayerService → LibMpvPlayer → libmpv
```

React 不得知道：mpv handle、native window handle、FFI pointer、libmpv lifecycle。

Domain API 与 libmpv implementation 必须分离。可简化目录，不可抹掉边界。

建议：

```
src-tauri/src/
  player/           # domain + PlayerService
    mpv/            # LibMpvPlayer
  commands/player.rs
  state/app_state.rs
```

Player Runtime 由 Tauri State 管理。libmpv 不得阻塞 Tauri UI thread。

## Player

接口：`open` / `play` / `pause` / `stop` / `seek` / `get_position` / `get_duration` / `set_volume` / `set_rate` / `get_state`。

预留、本阶段不实现：`set_audio_track` / `set_subtitle_track`。

状态用 enum，禁止互相矛盾的 bool：

`Idle | Loading | Ready | Playing | Paused | Ended | Error`

Rust Player Runtime 是权威来源。Zustand 只镜像 UI 状态，不要让 React 猜测播放状态。

Rust → React 用 **Tauri Channel**（或等价流式机制）推送，禁止每 10ms invoke 轮询。Position 约 100–250ms。

事件：`StateChanged` / `PositionChanged` / `DurationChanged` / `FileLoaded` / `Ended` / `Error`。

## Conventions

- 优先级：可运行 > 架构清晰 > 代码简洁 > UI 美观。
- 显式优于隐式；组合优于继承；不要为抽象而抽象。
- 业务代码禁止 `unwrap()` / `expect()`。
- `PlayerError` 至少区分：InitializationError / LoadError / UnsupportedMedia / NativeWindowError / PlaybackError / InvalidState / InternalError。
- 给前端的错误必须是 `{ code, message, details? }`，不展示 panic / raw FFI。
- 日志用 `tracing`。记录 init / open / play / pause / seek / state transition / errors / shutdown。不要刷 position。
- 生命周期必须清晰：init、shutdown、换文件、播放结束、退出。避免 use-after-free、重复实例、native handle / thread 泄漏。
- 优先硬件解码；失败则 fallback，不要因此无法播放。

## Phase 1 禁止

HTML video、Codex ACP、MCP、ASR、Whisper、FFmpeg extraction、Transcript、SQLite、RAG、embedding、AI Chat、OCR、Cloud、plugin system。

不要为“以后可能需要”提前实现。Native rendering 未跑通时，停止扩展其他功能。
