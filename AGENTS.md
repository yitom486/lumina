# Lumina — Agent Guide

AI Video Reader 桌面应用。

**当前焦点：Monorepo 目标架构迁移与 Online Source 收敛。当前项目仍处于迁移状态，
必须严格按照最初制定的目标架构与迁移计划推进，不得自行创建替代性的组织模式。**

完整计划见 `project.md` / `ROADMAP.md`。执行计划与状态在 `.plan/`（本地，不入库）。每完成一个可检查点，必须更新对应文档的 `status` 与 `STATUS.md`。实现前先读本文件与 `.cursor/rules/`（尤其 `errors.mdc`、`phase-5.mdc`）。

### 当前迁移约束

- 架构唯一依据是 `.plan/L0-monorepo-target-architecture.md` 与
  `.plan/L1-monorepo-migration.md`；L2 任务只能细化它们，不能替换它们。
- M0–M9 是已完成的检查点，不代表整个目标架构迁移已经结束；当前总体状态仍为
  `doing`，后续工作必须继续沿原定依赖方向、边界和迁移顺序推进。
- 不得为了目录对称强行移动跨域编排、Tauri/native、libmpv、IPC 或真实桌面状态；
  任何暂缓项必须在对应 `.plan/` 文档中记录原因和下一步，不得私自改成“已完成”。
- 每个新迁移批次都必须先落盘可复制的执行提示词与验收标准，完成后更新状态，
  通过功能/静态门禁后再提交；不做无计划的大范围重构。

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
cargo fmt --all --check
cargo test --workspace --lib
cargo clippy --workspace --all-targets -- -D warnings
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
- 章节目标只有两种来源：容器/在线源提供的真实章节，以及用户主动触发的 AI 语义分段。
  不再新增或扩展 soft/机械分段；现有 P6-M5 实现属于待下线迁移债务，迁移完成前不得把它
  当作新的产品能力继续设计。AI 分段必须结合字幕/台词与按需截帧，不能在打开视频时静默启动。

## UI 与 AI 实现约束

- UI 原语统一使用现有 Radix UI + shadcn/ui；不得在同一套设计系统中引入 Base UI。
- 颜色不得在业务组件中硬编码十六进制、RGB 或命名色。必须使用语义 CSS 变量；亮色和暗色
  两套 token 都要完整定义。媒体画面黑色属于播放器 surface 的特殊语义，不得借此绕过主题变量。
- 快捷 AI 操作只能引用版本化的任务提示词，不得把专有提示词散落在 React 组件字符串中。
  提示词仓库、章节 Agent、输出校验、增量重试和 SQLite 数据层的执行约束见
  `.plan/L2-chapter-agent-data-pipeline.md`。
- AI 结构校验失败时，重试消息只能增量追加本次校验报告；同一校验错误最多重试三次，超过
  上限必须失败并保留原因。只有 Agent 会话因传输故障丢失时，才允许使用完整 bootstrap
  prompt 恢复新会话。底层技术细节仍只能进入 `details`/日志，不能直接展示给用户。

## Conventions

- 优先级：可运行 > 架构清晰 > 代码简洁 > UI 美观
- 业务代码禁止 `unwrap()` / `expect()`
- 给前端的错误必须是 `{ code, message, details? }`；底层错误只进 `details` / 日志
- 日志用 `tracing`；不要刷 position
- TanStack Query key 跨文件复用时走各 feature 的 `queries.ts` 工厂，不手写字面量
- 不与其他产品强行统一 lockfile；对齐领域模型与错误形状即可
- Vendor 原件只读：`packages/ui` 内 shadcn 拷贝以上游 default 风格为准，禁止直接修改；
  定制只允许调用方 `className`（经 `cn` 合并）或外层转接拷贝；包 Radix 原语的 wrapper
  必须透传 `ref`（`asChild` 链的硬性契约，丢 ref 会静默杀死浮层定位）

## UI design references

- 主界面视觉参考图：`ui/lumina-desktop-overview-reference.png`
- 文稿阅读工作区参考图：`ui/lumina-transcript-reading-reference.png`
- 时间戳笔记工作区参考图：`ui/lumina-notes-workspace-reference.png`
- 设置与资源管理工作区参考图：`ui/lumina-settings-workspace-reference.png`
- 通用聊天与富输出参考图：`ui/lumina-general-chat-rich-output-reference.png`
- AI 观剧流与自由聊天 Tab 参考图：`ui/lumina-ai-watch-feed-reference.png`
- 无章节视频的章节工作区参考图：`ui/lumina-chapter-generation-reference.png`
- AI 与自动化设置参考图：`ui/lumina-ai-automation-settings-reference.png`
- UI 交互与渲染约定：`ui/README.md`
- 设计方向：深蓝石墨背景、蓝青色 AI 状态光感、橙色播放操作强调，整体保持科技简约与低干扰。
- 参考图只定义视觉语言和信息层级，不改变原生播放器约束：`libmpv` HWND 必须与 WebView UI 使用并排布局，文稿、笔记、章节与 AI 面板不得依赖覆盖 HWND 的透明 WebView 层。
