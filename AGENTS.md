# Lumina — Agent Guide

AI Video Reader 桌面应用。

**当前阶段：Phase 4（按需 ASR）**；Phase 1–3（native playback / media inspect / 字幕文稿）已完成。

完整计划见 `project.md`。执行计划与状态在 `.plan/`（本地，不入库）。每完成一个可检查点，必须更新对应文档的 `status` 与 `STATUS.md`。实现前先读本文件与 `.cursor/rules/`。

## Tech Stack

- Desktop: **Tauri 2**
- Backend: **Rust**
- Frontend: **React + TypeScript + Vite**
- Package manager: **pnpm**
- UI: **Tailwind CSS + shadcn/ui**
- Client state: **Zustand**
- Async/server state: **TanStack Query**
- Player: **libmpv**（`libmpv2`）
- Window: Tauri Window + `HasWindowHandle` / `raw-window-handle`
- Media tools: 项目本地 `ffprobe` / `ffmpeg`（`native/ffmpeg/`）
- ASR（可选）: 本地 `whisper-cli` + `ggml-*.bin`（`native/whisper/`，按需）

禁止用 HTML `<video>`、canvas 逐帧复制等方式伪装 native playback。

## Architecture

```
React → Tauri Command / Channel → PlayerService → LibMpvPlayer → libmpv
                              → MediaInspector → ffprobe
                              → SubtitleService → ffmpeg + parse
                              → AsrService（按需）→ ffmpeg wav + whisper-cli
```

React 不得知道：mpv handle、native window handle、FFI pointer、libmpv / whisper lifecycle。

## ASR（Phase 4）

- **非必须**：播放、字幕、文稿不依赖 ASR
- **按需**：仅用户点击「生成文稿 (ASR)」才查找并 spawn whisper-cli；启动 / Open **不**预加载模型
- 未配置时返回 `NotConfigured`，应用仍可用

## Conventions

- 优先级：可运行 > 架构清晰 > 代码简洁 > UI 美观
- 业务代码禁止 `unwrap()` / `expect()`
- 给前端的错误必须是 `{ code, message, details? }`
- 日志用 `tracing`；不要刷 position
- 有文本字幕时优先 Phase 3；不要强迫走 ASR
