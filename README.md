# Lumina — AI Video Reader

> 当前版本 0.3.0 · Windows x64 / macOS arm64 / Linux x64

Lumina 是一款桌面端 AI 视频阅读器。

它把“播放视频”和“阅读视频内容”放在同一个工作流里：你可以观看本地或在线媒体，打开可检索的字幕文稿，按时间点记录笔记，再在需要时让 AI 基于当前画面、台词和章节回答问题。

Lumina 的基础体验以本地为主：播放、字幕和笔记不依赖 AI；AI、在线媒体解析和语音转写都是可选能力，按需启用。

安装包发布在 [lumina-app Releases](https://github.com/yitom486/lumina-app/releases)。版本变更见 [CHANGELOG.md](CHANGELOG.md)，产品路线见 [ROADMAP.md](ROADMAP.md)。

## 主要能力

### 像阅读一样观看视频

- 播放、暂停、停止、跳转、调速、音量和音轨切换。
- 支持本地视频，也支持按需打开在线视频。
- 可识别同目录下的分集媒体，方便连续观看。

### 字幕与文稿

- 将内嵌字幕、外挂字幕和在线字幕统一为可检索文稿。
- 点击台词即可跳转到对应时间点；文稿也可以跟随播放或独立浏览。
- 支持章节导航；没有正式章节时，可以使用基于字幕停顿的机械分段。
- 字幕不可用时，可按需使用本地语音转写生成文稿。

### 时间点笔记与批注

- 在视频的具体时间点创建笔记，之后可以从笔记跳回原片。
- 保存台词引用和关键画面，帮助保留笔记的上下文。
- 将观看记录导出为 Markdown，继续在其他知识管理工具中整理。

### 可选的 AI 视频对话

- 只有主动发起会话时才启动 AI Agent，不影响普通播放和阅读。
- AI 可以在受控范围内读取播放状态、字幕窗口、章节、截图和已有批注。
- 回答中的时间点引用可以跳回视频进行核对。
- 默认支持 Codex profile，也可以配置其他 ACP Agent 或自定义命令。

### 媒体库与在线能力

- 为自选目录建立本地媒体库，识别电影、剧集和分集关系。
- 可选地使用 TMDB、Wikipedia 等在线信息补充作品资料。
- 可选地使用 yt-dlp 解析在线视频，或使用本地 Whisper 进行语音转写。

这些能力都不会改变原始视频文件；网络服务、凭证和可选运行时只在相关功能被使用时才需要。

## 适合怎样的使用方式

Lumina 特别适合以下场景：

- 看课程、访谈、纪录片时，需要边看边查台词和章节；
- 学习外语时，对照字幕、定位句子并保存例句；
- 观看长视频或系列内容时，希望持续积累带时间点的个人笔记；
- 希望 AI 理解“正在看的这一段”，而不是把整部视频手动复制到聊天窗口。

## 安装与开始使用

1. 打开 [lumina-app Releases](https://github.com/yitom486/lumina-app/releases)。
2. 下载对应系统的安装包：Windows 使用 MSI / NSIS，macOS 使用 DMG，Linux 使用 AppImage / deb。
3. 安装后打开 Lumina，选择本地视频即可开始播放和阅读。

终端用户不需要安装 Bun、Node、pnpm、Rust 或 Whisper，就可以播放视频、阅读字幕和记录笔记。可选的 AI、在线视频和语音转写能力会在使用时单独检查相应配置。

## 项目状态

Lumina 仍在持续开发中。播放、媒体探测、字幕文稿、按需 ASR、笔记和基础 ACP 对话已经形成主链路；媒体库、在线媒体和更完整的 AI Video Reader 体验仍在迭代。

请以 [ROADMAP.md](ROADMAP.md) 了解阶段目标，以发布页和 [CHANGELOG.md](CHANGELOG.md) 了解具体版本变化。

## 从源码运行

这一部分面向开发者，不是终端用户的安装要求。仓库使用 Bun 管理前端工作区，Rust 用于桌面端后端。

环境准备：

- Bun 1.3.14；
- Rust stable 与 Windows MSVC C++ Build Tools（Windows）；
- 对应平台的 WebView 运行时；
- 播放所需的本地原生依赖。具体维护说明见 [`apps/desktop/src-tauri/native/`](apps/desktop/src-tauri/native/) 下的 README。

```powershell
bun install --frozen-lockfile
bun run tauri dev
```

常用检查：

```powershell
bun run lint:all
bun run test
bun run check:rust
```

更详细的架构约束、错误处理规则和迁移计划请阅读 [AGENTS.md](AGENTS.md)、[project.md](project.md) 以及 [ROADMAP.md](ROADMAP.md)。各 Rust 领域 crate 的实现说明位于对应 `crates/*/src/README.md`。

## 隐私与本地数据

- 本地播放、字幕阅读和笔记可以离线使用。
- 凭证不会写入项目文件、媒体库索引或 AI prompt。
- 原始视频文件不会被 Lumina 重命名或改写。
- 请勿提交个人视频、笔记、Cookie、模型密钥、`.env` 或本地 native 二进制。

## 参与项目

欢迎通过 Issue 或 Pull Request 反馈问题和提出改进建议。提交代码前，请先阅读 [AGENTS.md](AGENTS.md) 中的架构边界、错误处理和测试要求。
