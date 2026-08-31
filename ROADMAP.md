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
| 8 | Unified GPU Composition（远期研究：视频画面与 Web UI 可组合叠加） |

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

## Phase 8 — Unified GPU Composition（远期）

### 背景与目标

当前播放器使用 libmpv 子 HWND 输出视频，WebView 内的 HTML/CSS 无法可靠覆盖该原生视频区域；`z-index`、React Portal、Radix/Base UI 均不能跨越 native window 边界。

Phase 8 的目标不是简单替换播放器，而是验证并逐步建立统一的 GPU 合成路径，使视频画面与 Web UI 能在同一合成树中正确叠加，同时保留硬件解码、播放稳定性和当前 HWND 后端的回退能力。

这是远期研究项，不阻塞 Phase 5–7。当前产品继续采用视频安全区、收缩视频矩形和画面外控制栏。

### 硬性约束

- 禁止把视频逐帧读回 CPU，再复制到 HTML canvas。
- 保留硬件解码；目标为零拷贝，不能实现时最多接受 GPU 内部纹理传递。
- React 不得接触 mpv handle、GPU device、共享纹理或 native window 生命周期。
- 当前 `WindowedHwnd` 播放后端必须保留到新路径通过完整验收，可在运行失败时回退。
- 先支持 Windows/WebView2；其它平台必须通过后端接口隔离，不伪装为已支持。
- 性能结论必须来自同一设备、同一媒体、同一窗口状态下的基准对比。

### 技术路线（按检查点推进）

#### 8.0 可行性 Spike / Go-No-Go

- 建立当前 HWND 后端基线：1080p60、4K60、窗口/全屏、单屏/多屏。
- 记录 CPU、GPU、显存、丢帧、首帧时间、seek 恢复时间和 A/V sync。
- 验证 libmpv Render API 与 Windows D3D11/DirectComposition/WebView2 合成的可行路径。
- 比较至少两条候选方案：native DirectComposition 合成树、可共享 GPU 纹理的 WebView 合成路径。
- 验证透明 WebView、输入命中、DPI、全屏、窗口缩放和设备丢失恢复。
- 产出 ADR 与最小原型；若需要 CPU 逐帧复制或破坏硬件解码，则判定 No-Go，继续 HWND 安全区方案。

#### 8.1 播放渲染后端抽象

- 在 Rust 内建立 `VideoRenderBackend` 边界。
- 保留 `WindowedHwnd`，新增实验性的 `CompositedGpu`。
- PlayerService 继续作为唯一上层入口；前端状态与命令不因后端变化而分叉。
- 加入能力检测、启动选择、失败回退和仅开发环境启用的诊断开关。

#### 8.2 GPU 合成管线

- 建立 mpv render context、GPU surface/texture 生命周期与帧调度。
- 避免 CPU readback；明确纹理所有权、同步栅栏和背压策略。
- 处理窗口 resize、DPI、最小化/恢复、显示器切换和 GPU device lost。
- 保持暂停画面、seek、变速、字幕和色彩空间行为与当前后端一致。

#### 8.3 Web UI 覆盖能力

- 允许控制条、Tooltip、Dropdown、Dialog 和 ChatDock 在视频像素上方显示。
- 验证鼠标命中、键盘焦点、拖拽、触控、无障碍和 Portal 行为。
- 去除只为 HWND 空域问题存在的临时安全区，但保留兼容后端布局。
- UI 组件库仍可独立选择 Radix 或 Base UI；渲染后端不依赖二者。

#### 8.4 播放能力与视觉回归

- 覆盖 H.264/H.265/AV1、软硬字幕、外挂字幕、音轨切换、倍速和全屏。
- 验证 SDR/HDR、色彩范围、旋转视频、不同宽高比及高 DPI。
- 增加长时间播放、频繁 seek、睡眠恢复、多屏切换和窗口反复缩放测试。
- 对 UI 覆盖、画面裁剪、黑边、撕裂和闪烁进行截图/人工回归。

#### 8.5 灰度发布与默认切换

- 首先作为实验开关发布，失败自动回退 `WindowedHwnd`。
- 收集匿名性能数据前必须另行完成隐私设计与用户授权；未完成时仅使用本地诊断。
- 达到性能和稳定性门槛后才考虑默认启用；至少保留一个稳定版本周期的旧后端。

### 暂定验收门槛

- 不存在 CPU 逐帧像素回读/复制。
- 同一测试媒体下，硬件解码保持启用，A/V sync 无可感知回退。
- 1080p60 与 4K60 的丢帧率不显著高于 HWND 基线。
- 稳态 CPU/GPU 开销原则上不高于基线 10%；若超过，必须有测量依据和明确收益评审。
- seek、首帧和全屏切换延迟不出现明显回退。
- HTML 浮层可稳定覆盖视频，焦点、点击、DPI、多屏和全屏全部通过验收。
- 新后端异常时能够无崩溃地退回 HWND 后端。

以上阈值是立项门槛，不是对当前未知实现的性能承诺；8.0 Spike 完成后应根据真实数据修订。

### 粗略投入

- 8.0 可行性验证：1–2 周。
- 8.1–8.3 原型与主链路：3–6 周。
- 8.4–8.5 兼容、性能与发布：2–4 周。

整体按单人投入约 6–12 周估算，若 Tauri/Wry/WebView2 暴露能力不足，可能需要维护平台补丁或改走 native DirectComposition，周期会进一步增加。
