# Lumina — Agent Guide

AI Video Reader 桌面应用。

**当前焦点：错误处理 + 单测地基**（Phase 1–4 功能已落地；Phase 5 ACP 暂缓）。

完整计划见 `project.md`。执行计划与状态在 `.plan/`（本地，不入库）。每完成一个可检查点，必须更新对应文档的 `status` 与 `STATUS.md`。实现前先读本文件与 `.cursor/rules/`（尤其 `errors.mdc`）。

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

## Error handling（必读）

```
Domain Error { code, message(zh), details? }
  → Command Result
  → invoke / Channel
  → formatPlayerError / errorMessage
  → UI
```

细则见 [`.cursor/rules/errors.mdc`](.cursor/rules/errors.mdc)：

- 用户只看中文 `message`；技术细节进 `details`
- 可恢复失败（如瞬时 seek）不要打成整机 `PlayerState::Error`
- 重 I/O 必须 `async + spawn_blocking`，避免卡 UI

## Testing

```bash
pnpm lint          # tsc
pnpm test          # 前端纯函数单测
cd src-tauri && cargo test --lib
```

新增/修改错误 code 或用户文案时，必须补或更新对应单测。

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
