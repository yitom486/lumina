# Changelog

## 0.2.4 — Private Preview

### 修复

- **窗口无法关闭**：移除会拦截 Tauri 原生关闭请求的前端监听；在 Rust 原生窗口层显式处理关闭与 `Alt+F4`，确保应用最迟在 1.5 秒内退出。

## 0.2.3 — Private Preview

### 修复

- **Windows 终端闪窗**：spawn `ffmpeg` / `ffprobe` / `whisper-cli` / ACP 子进程时统一 `CREATE_NO_WINDOW`，浏览文稿、加载字幕、ASR 时不再弹出黑窗口。
- **文稿重复加载**：字幕列表与文稿查询加 `staleTime: Infinity`，避免窗口聚焦时重复提取内嵌字幕。

### 媒体库

- **默认跟随播放目录**：打开视频后自动将所在文件夹设为媒体库扫描根目录；仍可用「选择目录」固定其它路径，或用「跟随播放目录」恢复默认。

## 0.2.2 — Private Preview

### 播放器 / 构建

- **macOS / Linux 原生视频面**：AppKit NSView（macOS）与 X11 子窗口（Linux）作为 libmpv `wid`；Linux Wayland 会话暂不支持。
- **libmpv 运行时 bundle**：安装包将 `native/mpv/runtime/` 打入 resources；Windows delay-load + 预加载，Unix 优先加载 bundle 内 dylib/so。
- **Release / CI**：GitHub Actions 三端 matrix（Windows MSI/NSIS、macOS DMG、Linux AppImage/deb）。

### 已知边界

- Linux 需 X11 或 XWayland；Wayland 原生会话会提示暂不支持。
- macOS / Linux 暂无视频面点击/双击事件（Windows 已有）。
- ffmpeg / whisper 等非 Windows 安装包路径仍待完善；字幕与 ASR 在 macOS/Linux 上可能需另行配置。

## 0.2.1 — Private Preview

### 修复

- **Windows 安装包**：将 `libmpv-2.dll` 打入 bundle resources，启动时 delay-load 预加载，修复安装后「找不到 libmpv-2.dll」无法启动。

本项目当前为私有预览阶段。版本说明描述的是已实现能力，不承诺稳定 API、跨平台支持或公开分发。

## 0.2.0 — Private Preview

### 媒体库与元数据

- 实验性本地媒体库：目录守护扫描、`.lumina` 索引、待匹配分组与人工标题兜底。
- 可配置的 OpenAI-compatible 文件名解析与 TMDb 候选确认；模型密钥与 TMDb Token 可保存到 Windows Credential Manager。
- 「验证配置」：分别以最小请求验证模型服务与 TMDb Token，仅展示业务化结果。
- 智能匹配支持复用 ACP Agent profile 或专用直接 API；Agent 解析使用独立、工具禁用、短生命周期会话。
- 已确认的电影/剧集写入 `movie.json`、`series.json` 与分集 JSON；维基 W4 刷新、过期提示与中文对照。
- TMDb 刷新与分集元数据精简；维基容错与角色解析。

### ACP 对话与 MCP 工具

- MCP 按需上下文：快照瘦身、series 预热节流、5 个 Lumina MCP 工具（播放/媒体库/分集索引/字幕窗口/截图）。
- 修复 MCP 环境变量挂载、分集索引解析与媒体库路径统一。
- 对话体验：本地历史、新建会话、Composer 模型选择、Markdown 渲染、工具报错中文提示。
- Agent 提示词：工具慎用原则；剧情优先字幕窗口、画面细节再用截图。
- 助手回复合并：去除重复英译块、minimal 模式仅展示工具轨迹。
- ACP 回复分段：工具调用前 agent 正文不进入最终答案（对齐 Inkdown 只保留最后一段 agent 回复）；工具执行中隐藏答案气泡。

### 播放器

- **会话恢复**：关闭时保存上次视频、目录与进度；冷启动自动 reopen 并 **暂停** 在保存位置；文件对话框默认上次目录。

### 构建与质量

- Bun 前端 lint、Vitest 单测/UI 测试；Rust `cargo test --lib` 与 clippy。
- Windows x64 debug bundle：MSI 与 NSIS 安装器。

### 已知边界

- 当前正式支持目标为 Windows x64。
- libmpv 使用 native 子 HWND；控件须位于视频安全区或侧栏。
- 本版本不包含代码签名、自动更新服务或公开分发。

## 0.1.0 — Private Preview

### 主要功能

- Windows 原生 libmpv 播放面：播放、暂停、停止、seek、音量、倍速、全屏和播放列表。
- 同目录媒体列表、断点续播、媒体信息、音轨/字幕切换、外挂字幕与文本字幕文稿。
- 容器章节导航、时间戳笔记和 Markdown 导出。
- 可选本地 ASR：按需调用 whisper；未安装不会阻断播放。
- 可选 ACP 对话：按需启动 Agent，默认 Codex profile 通过 `bunx @agentclientprotocol/codex-acp` 适配；未配置不会阻断播放、字幕或笔记。
- ChatDock、播放器和侧栏的 native HWND/WebView 图层隔离；全屏状态与视频原生窗口边界同步。

### 构建与质量门槛

- Bun 前端 lint、Vitest 单测/UI 测试和生产构建。
- Rust 格式检查、库测试与 `clippy -D warnings`。
- Windows x64 debug bundle：MSI 与 NSIS 安装器。

### 已知边界

- 当前正式支持目标为 Windows x64。
- libmpv 使用 native 子 HWND，普通 HTML/CSS、Portal、Tooltip 或 Dialog 不能可靠覆盖正在播放的视频像素；控件必须位于视频安全区或布局侧栏。统一 GPU 合成属于路线图 Phase 8 的远期研究项。
- 本版本不包含代码签名、自动更新服务或公开分发。
