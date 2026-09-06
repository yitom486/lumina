# Lumina — Agent Guide

AI Video Reader 桌面应用。

**当前焦点：Phase 5 ACP Client（可插拔 Agent；默认 Codex profile）**。

完整计划见 `project.md` / `ROADMAP.md`。执行计划与状态在 `.plan/`（本地，不入库）。每完成一个可检查点，必须更新对应文档的 `status` 与 `STATUS.md`。实现前先读本文件与 `.cursor/rules/`（尤其 `errors.mdc`、`phase-5.mdc`）。

## Tech Stack

- Desktop: **Tauri 2**
- Backend: **Rust**
- Frontend: **React + TypeScript + Vite**
- Package manager（**仅开发/CI**）: **bun** — 终端用户安装包不需要 bun / pnpm / Node 才能播放
- UI: **Tailwind CSS + shadcn/ui**
- Client state: **Zustand**
- Async/server state: **TanStack Query**
- Player: **libmpv**（`libmpv2`）
- Window: Tauri Window + `HasWindowHandle` / `raw-window-handle`
- Media tools: 项目本地 `ffprobe` / `ffmpeg`（`native/ffmpeg/`）
- ASR（可选）: 本地 `whisper-cli` + `ggml-*.bin`（`native/whisper/`，按需）
- ACP（可选）: **ACP Client** 按需 spawn 本机 Agent 进程（默认 Codex 适配器）

禁止用 HTML `<video>`、canvas 逐帧复制等方式伪装 native playback。

## Architecture

```
React → Tauri Command / Channel → PlayerService → LibMpvPlayer → libmpv
                              → MediaInspector → ffprobe（含 chapters）
                              → SubtitleService → ffmpeg + parse
                              → AsrService（按需）→ whisper-cli
                              → AcpService（按需）→ ACP Agent stdio JSON-RPC
                                   └ default: codex-acp → Codex App Server → Responses API
                              → NoteService → 本地 JSON + Markdown 导出
```

React 不得知道：mpv handle、native window handle、FFI pointer、libmpv / whisper / ACP 子进程 lifecycle。

## ACP 分层（必读）

| 层 | 是什么 | Lumina 关系 |
|----|--------|-------------|
| **ACP** | 开放协议（stdio JSON-RPC） | **产品稳定边界**（只做 Client） |
| **codex-acp** | TS/Node 适配器（可打成单文件） | 默认 Agent **启动命令**，不是领域逻辑 |
| **Codex App Server** | Codex harness 引擎 | 仅 Codex profile 内部使用；**不要**当主协议 |
| **模型 API** | 现以 **Responses API** 为主 | 经 Codex `config.toml`；非任意 Chat URL |

- **换模型 ≠ 换 Agent**：Codex 内换 provider（Responses）；换 Claude 等 = 换 ACP Agent 进程（profile）
- 终端用户**不必**装 bun/pnpm 才能播放；开发用 bun；产品不得把 bun 当用户运行时硬依赖

## Error handling（必读）

细则见 [`.cursor/rules/errors.mdc`](.cursor/rules/errors.mdc)。摘要：

1. **目标是隐藏实现，不是「翻译」**：用户看不到工具名 / stderr / serde / 路径等底层信息
2. 形状：`{ code, message, details? }` — UI **只展示** `message`；`details` 仅日志（前端默认不渲染）
3. 底层失败在域构造函数映射为**固定**业务 `message`；调用方只传 `details`
4. 用户输入校验可用具体中文（`bad_request` / `invalid`）
5. 前端只用 `formatPlayerError` / `errorMessage`，禁止拼「详情：」+ `details`

## Testing

```bash
bun run lint
bun run test
cd src-tauri && cargo test --lib
```

## ASR（Phase 4）

- **非必须**；仅用户点击才 spawn whisper-cli；未配置返回 `NotConfigured`

## ACP（Phase 5）

- **非必须**：播放 / 字幕 / 笔记不依赖 ACP
- **按需**：用户发起会话才 spawn 当前 Agent profile；启动 / Open **不**预连接
- 默认 profile：`codex`；可配置 `claude` / `custom`（command + args + env）
- 未配置 → `NotConfigured`，应用仍可用

## 笔记 / 章节

- 笔记：时间戳锚点 + Markdown 导出；不依赖 AI
- 章节：容器元数据优先；无章节且有字幕允许机械分段（标明非语义，禁主题生成）；无字幕仍静默

## Conventions

- 优先级：可运行 > 架构清晰 > 代码简洁 > UI 美观
- 业务代码禁止 `unwrap()` / `expect()`
- 给前端的错误必须是 `{ code, message, details? }`；底层错误只进 `details` / 日志
- 日志用 `tracing`；不要刷 position
- TanStack Query key 跨文件复用时走各 feature 的 `queries.ts` 工厂，不手写字面量
- 不与其他产品强行统一 lockfile；对齐领域模型与错误形状即可
