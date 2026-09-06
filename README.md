# Lumina — AI Video Reader

> 当前版本 0.3.0 · Windows x64 / macOS arm64 / Linux x64

Lumina 是一款桌面端 AI 观影阅读器：用原生 libmpv
播放本地视频，并把字幕文稿、章节、笔记与可选的 AI
对话放在同一个阅读工作流里。播放、字幕、笔记不依赖
任何在线服务与 AI；AI 能力全部按需启动。

安装包发布在 [lumina-app](https://github.com/yitom486/lumina-app/releases)；
版本变更见 [CHANGELOG.md](CHANGELOG.md)，路线图见 [ROADMAP.md](ROADMAP.md)。

## 功能

### 原生播放

- libmpv 经系统窗口句柄直接渲染，不使用 HTML `<video>` 或逐帧 canvas。
- 打开 / 播放 / 暂停 / 停止 / 进度跳转 / 音量 / 倍速 / 播放列表 / 断点续播。
- 同目录分集自动识别与切换；HEVC / AV1 走系统解码链路。
- Linux 需 X11 会话（Wayland 暂不支持原生视频面）。

### 文稿与阅读（P6）

- 字幕文稿：内嵌 / 外挂 / 在线字幕统一成可检索文稿，支持逐句定位与跳转。
- 可控跟读：跟随中 / 浏览中 / 关三态；手动滚动、选字不再被抢回；滚动永不 seek。
- 可验证引用：回答里的 `[mm:ss]` / `[第N集 · mm:ss]`
  全部来自工具实返，点引用跳播；不可验证的只显示文本。
- 快捷问：文稿每句「问」（解释这一段）、章节行「总结本章」，提问锚点恒为所问时刻。
- 系列阅读：媒体库内继续阅读（首个未完成）/ 下一集 / 手动完成；跨集引用默认防剧透。
- 章节：容器章节优先；无章节时按字幕停顿机械分段（标注非语义，不编造主题）。

### 笔记与批注

- 时间戳锚点笔记：列表浏览、点击跳转、一键导出 Markdown。
- 台词引用：手动或自动附带前后台词，导出保留引用块。
- 视频批注：AI 回答可内联确认存为批注；批注可附单帧截图（缩略图 + 点击 seek，
  删批注自动清理图片文件）。

### AI 对话（ACP，可选）

- 不预连接：只有用户发起会话时才按需启动本机 Agent 进程。
- 默认 Codex profile，可配 Claude / 自定义命令；换模型走 Responses 配置，
  换 Agent 即换启动进程。
- 对话能力（播放上下文、字幕窗口、截图、批注）全部经 MCP 工具受控调用；
  Cookie 与签名 URL 永不进入 prompt 与日志。
- 未配置时应用其余功能完全可用。

### 媒体库（实验性）

- 对自选目录建本地 `.lumina/` 索引，周期扫描发现新增；TMDb + 维基元数据补全。
- 智能匹配：文件名解析 + TMDb 候选约束确认；可用已配 ACP Agent 或专用直连 API。
- 模型 Key / TMDb Token 存当前用户 Credential Manager，不进项目、索引与日志，
  界面不回显；环境变量仅作开发与 CI 备用。

### 在线视频与 ASR（可选）

- 在线解析：按需调用 yt-dlp（官方 stable + SHA-256 校验 + 可恢复替换），
  支持 cookies.txt / 浏览器 Cookie；失败按登录态分类给中文提示。
- 本地转写：仅用户点击才调本地 whisper；未配置返回未配置，不影响播放。

### 自动更新与日志

- 内置 updater：从 lumina-app 的 `latest.json`
  检查三平台签名包；标题栏可一键打开日志目录（daily rotation + panic hook）。

## 安装（终端用户）

1. 打开 [lumina-app Releases](https://github.com/yitom486/lumina-app/releases)，
   下载对应系统的安装包：Windows x64 用 MSI / NSIS，macOS 用 DMG，
   Linux x64 用 AppImage / deb。
2. 安装后直接打开本地视频即可使用。

> 终端用户不需要安装 Bun、Node、pnpm、Rust 或 whisper
> 就能播放、看文稿、记笔记。Bun 只在下面「从源码构建」时需要。

## 从源码构建（开发者）

使用前必须先安装 **Bun 1.3.14**（仓库 `packageManager` 锁定版本，
CI 与所有前端命令都经 Bun 运行）：

```powershell
# Windows（其他系统见 https://bun.sh/docs/installation）
powershell -c "irm bun.sh/install.ps1 | iex"
bun --version  # 应为 1.3.14
```

还需要：Rust stable + MSVC C++ Build Tools、WebView2 Runtime（Windows 11
通常自带）、项目本地 libmpv 开发包（见
[src-tauri/native/mpv/README.md](src-tauri/native/mpv/README.md)；
`ffmpeg` / `whisper` / ACP Agent 都是可选能力，见各自 `src-tauri/native/`
目录下的 README）。

```powershell
bun install --frozen-lockfile
bun run tauri dev
```

若 Rust 未加入 `PATH`，当前 Windows 会话可临时加入：

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
```

### 常用命令

```powershell
bun run lint:all   # tsc + oxlint
bun run test       # 前端全量（vitest）
bun run build      # 前端构建

bun run check:rust # Rust 门：fmt + check + test + clippy(-D warnings)

bun run tauri build --debug  # Windows debug 打包（MSI + NSIS）
```

Rust 另有 `cd src-tauri && cargo test --lib`。发版走
`.github/workflows/release.yml`：推送 `v*` tag 自动跑质量门、三平台构建、
签名并上传到 lumina-app（含合并版 `latest.json`）；也支持 Actions
手动 `workflow_dispatch` 指定 tag 重跑。发版前请读
[CHANGELOG.md](CHANGELOG.md) 并手工安装一次产物做播放冒烟。

## 应用内关于

标题栏右侧 ⓘ 按钮打开「关于」：显示当前版本号、一句话介绍，
并可一键打开 lumina-app 发布页下载更新。

## 参与与约束

- 不要提交个人视频、笔记、`.env`、模型密钥、Cookie 与项目本地 native 二进制。
- 给前端的错误只展示中文业务 `message`；业务代码禁 `unwrap()` / `expect()`；
  日志用 `tracing`。细则见 [AGENTS.md](AGENTS.md)。
