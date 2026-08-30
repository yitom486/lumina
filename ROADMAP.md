# ROADMAP

| Phase | Focus |
|-------|--------|
| 1 | Native Player（**完成**） |
| 2 | FFmpeg Media Inspection（**完成** — 项目本地 ffprobe + MediaInfo UI） |
| 3 | Subtitle / Transcript（**完成** — 文本字幕文稿 + 点击 seek） |
| 4 | ASR（**完成** — 按需 whisper-cli，不预加载） |
| 5 | Codex ACP（按需本机 CLI；未配置不影响播放） |
| 5b | 笔记 + Markdown 导出（时间戳锚点） |
| 5c | 容器章节导航（仅元数据；无则静默） |
| 6 | AI Video Reader |
| 7 | Multimodal Video Understanding |

## Phase 1 验收摘要

- Native 画面：子 HWND + `wid`，非 HTML `<video>`
- 控制：Open / Play / Pause / Stop / Seek / Volume / Rate
- 状态：Rust 权威，Channel → Zustand 镜像
- 检查：`cargo check` / `clippy -D warnings` / `bun run lint`

## Phase 3 摘要

- `ffmpeg.exe` 与 ffprobe 同目录（gitignore）
- `SubtitleService`：列轨、抽出、解析 SRT/ASS/VTT
- `TranscriptPanel`：高亮 + 点击 seek；支持外挂字幕
- 位图字幕可上画面；文稿需文本轨或 ASR

## Phase 4 摘要

- ASR **非必须**；`asr_status` / `asr_transcribe` 仅按需
- 启动不加载模型；点击按钮才 spawn whisper-cli
- 未配置时播放/字幕仍可用

## Phase 5 — ACP Client（可插拔 Agent）

- Lumina **只做 ACP Client**；不直连 Codex App Server 协议
- **默认 profile：`codex`**：spawn `bunx @agentclientprotocol/codex-acp`（可回退单文件）→ 内部 Codex App Server → **Responses API**
- **其它 harness**（Claude 等）：另配 AgentProfile（command/args/env），换进程而非锁死 App Server
- Codex 内换模型：`config.toml` provider，须 Responses 兼容（Chat Completions 已弃用）
- 未配置 → `NotConfigured`；播放 / 字幕 / 笔记仍可用
- **bun 仅开发/CI**；不要求终端用户装 bun 才能播放
- 不把 Codex / Node 打进默认安装包

## 笔记 + 导出

- `Note { id, mediaPath, positionMs, body, createdAt, updatedAt }`
- 本地 JSON 存储；侧栏列表；点击 seek
- 导出 Markdown（`[mm:ss] body`）

## 章节（仅元数据）

- ffprobe `-show_chapters`；有 chapters 才显示导航
- 一般无章节视频：**不**自动生成断点/主旨（留给更晚 AI Reader）

## 包管理器说明

| 场景 | 工具 |
|------|------|
| 开发 Lumina 前端 | **bun**（仓库约定） |
| 终端用户运行安装包 | 无需 bun / pnpm |
| 可选 ACP | 本机 Agent（默认 codex-acp；bunx 路径自带 Codex，或单文件/PATH） |
| 另一款产品 | 可同用 bun；对齐领域模型，不必强行同一仓库 |
