# Lumina — Agent Guide

AI Video Reader 桌面应用。

**当前焦点：Phase 5 Codex ACP**（Phase 1–4 与错误地基已落地；随后笔记/导出与容器章节）。

完整计划见 `project.md` / `ROADMAP.md`。执行计划与状态在 `.plan/`（本地，不入库）。每完成一个可检查点，必须更新对应文档的 `status` 与 `STATUS.md`。实现前先读本文件与 `.cursor/rules/`（尤其 `errors.mdc`、`phase-5.mdc`）。

## Tech Stack

- Desktop: **Tauri 2**
- Backend: **Rust**
- Frontend: **React + TypeScript + Vite**
- Package manager（**仅开发/CI**）: **pnpm** — 终端用户安装包不需要 pnpm / bun / Node
- UI: **Tailwind CSS + shadcn/ui**
- Client state: **Zustand**
- Async/server state: **TanStack Query**
- Player: **libmpv**（`libmpv2`）
- Window: Tauri Window + `HasWindowHandle` / `raw-window-handle`
- Media tools: 项目本地 `ffprobe` / `ffmpeg`（`native/ffmpeg/`）
- ASR（可选）: 本地 `whisper-cli` + `ggml-*.bin`（`native/whisper/`，按需）
- ACP（可选）: 本机 `codex-acp` + Codex CLI（PATH 或设置），按需 spawn

禁止用 HTML `<video>`、canvas 逐帧复制等方式伪装 native playback。

## Architecture

```
React → Tauri Command / Channel → PlayerService → LibMpvPlayer → libmpv
                              → MediaInspector → ffprobe（含 chapters）
                              → SubtitleService → ffmpeg + parse
                              → AsrService（按需）→ ffmpeg wav + whisper-cli
                              → AcpService（按需）→ codex-acp stdio JSON-RPC
                              → NoteService → 本地 JSON + Markdown 导出
```

React 不得知道：mpv handle、native window handle、FFI pointer、libmpv / whisper / ACP 子进程 lifecycle。

## Error handling（必读）

```
Domain Error { code, message(zh), details? }
  → Command Result
  → invoke / Channel
  → formatPlayerError / errorMessage
  → UI
```

细则见 [`.cursor/rules/errors.mdc`](.cursor/rules/errors.mdc)。

## Testing

```bash
pnpm lint          # tsc
pnpm test          # 前端纯函数单测
cd src-tauri && cargo test --lib
```

## ASR（Phase 4）

- **非必须**；仅用户点击才 spawn whisper-cli；未配置返回 `NotConfigured`

## ACP（Phase 5）

- **非必须**：播放 / 字幕 / 笔记不依赖 ACP
- **按需**：用户发起会话才查找并 spawn `codex-acp`；启动 / Open **不**预连接
- 未配置时返回 `NotConfigured`，应用仍可用
- 不要求终端用户安装 pnpm；开发者用 pnpm 构建前端即可

## 笔记 / 章节

- 笔记：时间戳锚点 + Markdown 导出；不依赖 AI
- 章节：只读容器元数据；无则静默不展示（不做 AI 断点）

## Conventions

- 优先级：可运行 > 架构清晰 > 代码简洁 > UI 美观
- 业务代码禁止 `unwrap()` / `expect()`
- 给前端的错误必须是 `{ code, message, details? }`
- 日志用 `tracing`；不要刷 position
- 有文本字幕时优先 Phase 3；不要强迫走 ASR
- 不与其他产品统一 bun/pnpm；对齐领域模型与错误形状即可
