# Lumina — Agent Guide

AI Video Reader 桌面应用。当前做 **Phase 2：FFmpeg Media Inspection**（Phase 1 Native Playback 已完成）。

完整计划见 `project.md` / `ROADMAP.md`。执行计划与状态在 `.plan/`（本地，不入库）。每完成一个可检查点，必须更新对应文档的 `status` 与 `STATUS.md`。实现前先读本文件与 `.cursor/rules/`。

## Tech Stack

- Desktop: **Tauri 2**
- Backend: **Rust**
- Frontend: **React + TypeScript + Vite**
- Package manager: **pnpm**
- UI: **Tailwind CSS + shadcn/ui**
- Client state: **Zustand**（播放器镜像）
- Async/server state: **TanStack Query**（媒体探测等一次性请求）
- Player: **libmpv**（`libmpv2`）
- Media inspect: **ffprobe**（项目本地 `src-tauri/native/ffmpeg/`，CLI JSON，不链 libav）
- Window: Tauri Window + `HasWindowHandle` / `raw-window-handle`

禁止用 HTML `<video>` 伪装 native playback。

## Architecture

```
React → Tauri Command / Channel → PlayerService → LibMpvPlayer → libmpv
                 ↘ media_inspect → MediaInspector → ffprobe
```

React 不得知道：mpv handle、native window handle、FFI pointer、ffprobe 进程细节。

Domain API 与实现分离：

```
src-tauri/src/
  player/           # playback domain + mpv
  media/            # inspection domain + ffprobe
  commands/
  state/
```

## Phase 2 范围

In：容器/流元数据、结构化错误、打开后 UI 摘要。  
Out：转码、抽帧流水线、ASR、Transcript、SQLite、RAG、ACP、AI Chat。

## Conventions

- 优先级：可运行 > 架构清晰 > 代码简洁 > UI 美观
- 业务代码禁止 `unwrap()` / `expect()`
- 前端错误形状：`{ code, message, details? }`
- 日志用 `tracing`
- 本地 native 二进制 gitignore，保留 README + VERSION
