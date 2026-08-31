# Lumina

> 0.1.0 · Windows x64 私有预览版

Lumina 是一款桌面端 AI Video Reader：用原生 libmpv 播放本地视频，并把字幕、文稿、笔记、章节和可选 AI 对话放在同一个阅读工作流里。

本仓库目前不适合公开发布。请将其保存在私有 GitHub 仓库；不要提交个人视频、笔记、`.env`、Codex 登录配置，或项目本地 native 二进制。

## 0.1.0 包含什么

- 原生播放：libmpv 通过 Windows 子 HWND 渲染，不使用 HTML `<video>` 或逐帧 canvas。
- 播放控制：打开、播放/暂停、停止、进度跳转、音量、倍速、播放列表和断点续播。
- 媒体阅读：ffprobe 媒体信息、内嵌/外挂字幕、文本字幕文稿、章节导航。
- 笔记：时间戳锚点、列表浏览、跳转和 Markdown 导出。
- 可选 ASR：仅用户触发时调用本地 whisper，不配置时不影响播放。
- 可选 AI 对话：通过 ACP 按需启动本机 Agent；默认 Codex 使用 `bunx @agentclientprotocol/codex-acp`，未配置时不影响其他功能。
- 原生视频图层约束：播放器、普通侧栏与 ChatDock 使用互斥布局列，HTML 浮层不会覆盖 native 视频 HWND。

完整路线图见 [ROADMAP.md](ROADMAP.md)，版本变更见 [CHANGELOG.md](CHANGELOG.md)。

## 媒体库元数据（实验性）

侧栏的「媒体库」可对用户选择的目录建立本地 `.lumina/` 索引，并以周期扫描发现文件变化。待匹配的剧集或电影可先输入作品名，或启用智能匹配：小模型只接收文件名和相对目录名，再由 TMDb 候选结果约束确认。

模型地址、模型 ID、扫描目录和轮询周期保存在本地 WebView 设置；密钥不保存到项目或 `.lumina`。请在启动 Lumina 的环境中设置：

```powershell
$env:LUMINA_METADATA_MODEL_API_KEY = "你的模型密钥"
$env:LUMINA_TMDB_ACCESS_TOKEN = "你的 TMDb Read Access Token"
```

可在「智能匹配设置」中修改环境变量名、OpenAI-compatible 模型地址和模型 ID。未配置模型或 TMDb 时，播放、字幕和笔记仍完全可用。

## 开发环境

当前发布目标是 Windows x64。开发机需要：

- Bun 1.3.14
- Rust stable + MSVC C++ Build Tools
- WebView2 Runtime（Windows 11 通常已自带）
- 项目本地 libmpv 开发包：参见 [src-tauri/native/mpv/README.md](src-tauri/native/mpv/README.md)

`ffmpeg`、`whisper` 与 ACP Agent 都是可选能力；详细约定参见各自 `src-tauri/native/` 目录下的 README。

```powershell
bun install --frozen-lockfile
bun run tauri dev
```

若 Rust 未加入 `PATH`，当前 Windows 会话可临时加入：

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
```

## 验证与打包

```powershell
bun run lint
bun run test
bun run build

Push-Location src-tauri
cargo fmt --check
cargo test --lib
cargo clippy --all-targets -- -D warnings
Pop-Location

bun run tauri build --debug
```

Windows debug bundle 会生成 MSI 和 NSIS 安装器。打包后的终端用户不需要安装 Bun、Node、pnpm 或 whisper 才能播放视频。

## 私有发布

- `.github/workflows/ci.yml`：在 Windows 上验证前端、Rust、libmpv 链接和 debug bundle，并上传仅仓库成员可见的 Actions artifact。
- `.github/workflows/release.yml`：推送 `v0.1.0` 这类 tag 时创建 **draft** GitHub Release 并附带安装器；仓库保持私有时，Release 与资产也只对有权限的成员可见。
- CI 会从 [VERSION](src-tauri/native/mpv/VERSION) 下载固定版本的 libmpv 开发包；二进制本身不会进入 Git。

发布前请阅读 [CHANGELOG.md](CHANGELOG.md)，并手工安装一次生成的 MSI/NSIS 包进行播放冒烟测试。
