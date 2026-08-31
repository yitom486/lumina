# Changelog

本项目当前为私有预览阶段。版本说明描述的是已实现能力，不承诺稳定 API、跨平台支持或公开分发。

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
