# 章节 Agent 与真实 Tauri 流程 — F-4 验收清单

本清单验收的是 Windows 桌面真实链路：原生播放器、字幕/截帧证据、按需启动的独立 ACP 章节会话、输出校验与增量重试、SQLite 持久化以及 AI 观剧流投影。它不是 Playwright DOM E2E，也不把 mock Agent 或浏览器 `<video>` 当作通过条件。

## 当前工作树的边界

截至本分支验收准备时，章节 worker 已有本地媒体探测、字幕窗口和真实 ffmpeg 截帧的执行缝隙；章节 ACP 会话、校验重试、SQLite 表、观剧流任务卡和重启恢复仍属于 L2 Batch B/C/D 的待实现验收面。若 UI 没有下述入口或数据库尚未有对应 migration，应记录为 **未覆盖/阻断**，不能用机械 soft segment、自由聊天消息或人工插库代替通过。

## 1. 前置条件

- Windows 10/11，使用本分支的工作树；不要切换分支或提交本次改动。
- `bun` 可用；开发启动命令是仓库根目录的 `bun run tauri`。旧文档中的 `bun tauri dev` 不是本仓库入口。
- `apps/desktop/src-tauri/native/ffmpeg/ffmpeg.exe` 与 `ffprobe.exe` 已准备好（或明确传入等价路径）；安装包验收还要确认 Tauri resources 中的 `ffmpeg/` 资源存在。
- libmpv runtime 和 Windows 原生播放依赖已准备好；播放器必须实际显示 native surface。
- Agent Settings 已配置一个可运行 profile（默认 `codex`），并完成所需登录/授权。Agent 是按需启动的；打开应用或媒体不应提前 spawn。
- 关闭会持有同一 Codex rollout/session 的其他桌面客户端，避免 writer lock 把“恢复失败”伪装成 Lumina 数据丢失。
- 记录本轮测试的应用数据目录、媒体路径、profile、模型和 reasoning 档位；不要把 token、完整 stderr 或用户目录中的敏感内容贴入缺陷。

## 2. 生成并验证 deterministic fixture

脚本只生成输入媒体并调用 ffprobe 校验，不启动 Tauri、ACP 或 Agent，因此可以在没有外部 Agent 的机器上重复运行：

```powershell
$out = Join-Path $env:TEMP "lumina-chapter-agent-e2e"
bun scripts/chapter-agent-e2e-fixture.mjs --out $out
bun scripts/chapter-agent-e2e-fixture.mjs --verify (Join-Path $out "chapter-agent-fixture.mkv")
```

输出的 `chapter-agent-fixture.mkv` 具有约 12 秒视频、4 条内嵌中文字幕、无容器 chapters；同目录的 `.srt` 和 `.manifest.json` 只用于审计。若 native 工具未在默认位置，使用 `--ffmpeg <path> --ffprobe <path>`，不要改生产配置。

脚本通过标准：JSON 报告 `status=ok`，`videoStreams >= 1`、`subtitleStreams >= 1`、`chapters = 0`、`durationMs >= 10000`。失败分类为 `fixture/environment`，先修复工具路径或编码器，不进入 UI 流程。

## 3. 启动与媒体基线（人工 + 可复制日志）

1. 在仓库根目录运行 `bun run tauri`，等待窗口完全启动。
2. 用真实文件打开 fixture；不要在测试前手动写入章节或选择已有章节视频。
3. 在章节面板确认：探测完成后没有容器章节；不能出现“按字幕停顿机械分段”作为产品结果。应显示无真实章节时的用户主动 AI 分段入口。
4. 在文稿/字幕面板确认内嵌字幕可选且能读到 4 条 fixture 台词。播放、拖动和暂停不能启动 Agent。
5. 用任务管理器或开发日志记录打开媒体前后的 Agent 进程数量；打开/重启只允许播放器和普通媒体服务初始化，不允许章节 Agent 预连接。
6. 检查播放器矩形：视频来自 libmpv native HWND；章节、文稿、AI 面板是 sibling layout。不要把透明 WebView、中心弹窗或“浮层覆盖播放器”记为通过条件。

当前 worker 可观察到的成功日志应类似：

```text
chapter evidence prepared task_key=<stable-key> transcript_windows>0 screenshots>0
```

日志中的 `task_key` 必须能在后续作业/章节记录中关联，但路径、stderr、JSON-RPC 和进程细节只进日志/details，不得成为 UI message。

## 4. 用户触发 AI 分段（真实 Tauri）

1. 在无容器章节的章节工作区点击「开始 AI 分段」；这是本流程唯一的启动动作。重复点击同一媒体/episode key，应该复用或拒绝重复作业，不应产生第二套章节资产。
2. 立即记录返回的任务身份和 UI 状态：任务进入 queued/running，且任务卡属于 **AI 观剧流**，不是自由聊天 turn。
3. 确认新建的是独立 ACP session：它可以使用当前 Agent profile、工作目录和 Lumina MCP，但不能复用主聊天 session，也不能把章节 prompt/产物写入自由聊天历史。
4. 在 Agent/MCP 事件或开发日志中确认按时间轴逐步读取字幕窗口，并至少成功请求一次 `lumina_capture_frames`；只读完整段字幕后凭空生成章节、或没有任何画面证据，不通过。
5. 检查专有任务是版本化 `chapter_segment` prompt；快捷按钮只传任务 ID、参数和用户补充，不把专有 prompt 文本散落到 React 可见消息中。
6. 章节 Agent 的会话生命周期由 Rust/ACP 管理。React 不应接触 HWND、FFI pointer、子进程生命周期或原始协议内容。

建议记录的事件序列（名称可随实现调整，但语义必须保留）：

```text
chapter task queued
chapter agent session created (session is not main chat)
transcript window requested: ...
frame evidence captured: ...
chapter output received
chapter output validated
chapter task persisted
watch feed projection updated
```

## 5. 输出校验与三次增量重试

需要一个可控的校验失败输入或测试 Agent 输出；不要通过改生产代码、直接改数据库或伪造 DOM 事件制造结果。失败注入应只让结构化输出缺少一个硬字段，随后恢复为合法输出。

- 首次输出必须先进入领域校验器，再进入富组件归一化和 UI。
- 每次失败都保存结构化报告：`error_code`、字段路径、实际值摘要、期望形状、原因、修复建议。
- 修正消息只能追加**本次**校验报告，不能重发初始专有 prompt、旧完整上下文或重复用户指令。
- 同一校验错误最多重试三次；第三次仍失败时作业为 `failed`，保留最终原因，不得无限重试。
- ACP 传输丢会话是另一条恢复路径，才允许 bootstrap 新会话；不能把传输恢复计入内容校验的三次额度。
- UI 只显示业务失败提示，例如“章节分析未完成，请重试”；不得显示工具名、stderr、路径、serde、JSON-RPC 或 exit code。

预期日志/状态断言：

```text
attempt=0 validation=failed
attempt=1 mode=incremental validation=failed
attempt=2 mode=incremental validation=failed
attempt=3 mode=incremental validation=success   # 可修复样例
```

或在不可修复样例中，`attempt=3` 后直接 `status=failed`；不能出现 `attempt=4`。记录每条消息的摘要/哈希，确认增量消息没有重复 bootstrap 内容。

## 6. SQLite 投影与 AI 观剧流

以下是 L2 计划定义的逻辑断言；实际 migration 落地后用只读查询或项目提供的诊断命令核对，不能为了通过验收手工建表。SQLite 由 Rust 服务访问，React/ACP 不直接读写。

| 断言 | 通过条件 |
|---|---|
| `agent_tasks` / `agent_attempts` | 只有一个稳定 task identity；独立 session、prompt version、attempt 次数、状态、校验报告和最终结果可追溯 |
| `episodes` / `episode_chapters` | fixture 绑定到稳定媒体/episode；章节来源为 `ai`，边界单调、在时长内、无重叠 |
| `chapter_assets` | 截图路径、哈希、时间点、尺寸和来源存在；文件在应用数据目录，SQLite 不存图片 BLOB；重复点击不复制资产 |
| `chapter_revisions` | 接受的章节版本与前情/主线/展望/看点/问题引用一致；失败版本仍可审计 |
| `watch_feed_items` | 观剧流卡片只引用章节版本和任务状态；不产生自由聊天 turn |

在 **AI 观剧流** Tab 验收：任务卡先显示 queued/running，再显示产物/失败状态；能展开章节、字幕引用、截图引用和跳转锚点；富组件只渲染白名单结构。切到 **自由聊天** Tab，原有 turns 数量、内容、session/history 不增加章节任务消息。

## 7. 重启恢复、失败重试与普通聊天隔离

### 成功作业

- 关闭应用并等待进程树退出，再用 `bun run tauri` 启动。
- 重新打开同一个 fixture，确认已接受章节、资产索引和观剧流卡片从持久层恢复。
- 启动期间没有自动新建章节 Agent session；只有用户再次点击需要计算的动作才 spawn。
- 章节内容、截图数量、task identity 不重复；恢复的是结果/状态，不是把章节伪装成聊天历史。

### 中断作业

- 在 Agent 仍运行时正常关闭应用，重启后确认任务被标为可恢复/失败（以产品状态机为准），而不是静默丢失或假装成功。
- 点击失败任务的「重试」后只新增允许的 attempt，复用同一 task identity；校验重试仍受三次上限约束。
- 传输故障恢复必须有新 session 的原因记录；内容校验失败不得借此重置次数。

### 普通聊天回归

- 在章节任务前后各发送一条普通聊天消息，确认普通聊天仍能创建/恢复自己的 session。
- 自由聊天 turns、历史列表、主聊天 session 的 `updatedAt` 不因章节进度事件变化。
- 章节任务卡不出现在自由聊天消息气泡中，章节 Agent 的工具活动不污染普通聊天轨迹。

## 8. 原生窗口人工验收边界

下列项目必须人工在 Windows 桌面确认，禁止伪造 Playwright DOM E2E：

- libmpv HWND 与 WebView sibling 布局的 bounds、重排、最小化/恢复和窗口关闭行为。
- 播放器画面是否被章节/文稿/AI DOM 覆盖，面板打开后 native surface 是否仍可播放。
- Tauri 窗口重启后 HWND 重建、Agent 子进程树退出以及再次打开媒体的顺序。
- 真实 ffmpeg 截帧文件是否可打开、时间点是否对应播放轴；脚本只能验证存在性/格式和 ffprobe 元数据。
- Agent 权限审批、外部登录、writer lock、Windows Job Object 进程树和系统休眠/杀进程行为。

## 9. 清理

1. 正常结束章节 Agent 会话，确认没有孤儿 `codex-acp`/Node/Codex 进程。
2. 退出 Tauri 后，删除本次生成的临时 fixture 目录；先确认路径只位于 `%TEMP%\lumina-chapter-agent-e2e`，再执行：

```powershell
$out = Join-Path $env:TEMP "lumina-chapter-agent-e2e"
if (Test-Path -LiteralPath $out) { Remove-Item -LiteralPath $out -Recurse -Force }
```

3. 不删除应用数据库、Agent rollout、用户字幕或仓库中的 native 原件；需要隔离数据时使用新的临时应用数据目录，并在记录中写明路径。

## 10. 失败分类与报告模板

| 分类 | 典型症状 | 需要附带的证据 |
|---|---|---|
| fixture/environment | ffmpeg/ffprobe、mpv runtime 缺失；fixture 有 chapters 或无字幕 | fixture 脚本 JSON、工具路径、OS/build |
| media/subtitle | 打开失败、字幕选择为空、窗口数为 0 | 媒体 manifest、UI 状态、脱敏日志 |
| native/HWND | 播放器黑屏、面板遮挡、重排失效 | 人工录屏/截图、窗口尺寸、复现步骤 |
| agent/config | 未配置、spawn 失败、权限/登录失败 | profile id、业务错误 code/message、details 仅开发日志 |
| ACP/lifecycle | 未按需启动、session 串线、孤儿进程、恢复 writer lock | session/task 关联、进程树、时间线 |
| evidence | 未请求字幕窗口/截帧，或引用不属于当前媒体 | 工具调用摘要、asset 索引、时间点 |
| validation/retry | 重发 bootstrap、超过三次、失败原因丢失 | attempt 序列、校验报告摘要/哈希 |
| persistence/projection | 重启丢失/重复、任务写入自由聊天、观剧流缺卡 | 只读 SQLite 查询、重启前后 task/asset 计数 |
| error contract | UI 显示 stderr、路径、JSON-RPC 或工具名 | UI 截图、业务 error JSON（脱敏） |

报告至少包含：日期、commit/工作树标识、Windows/Tauri 构建、fixture manifest、profile（不含凭据）、媒体 task identity、通过/失败步骤、日志时间范围、SQLite 只读断言和是否清理。

## 11. 与旧 manual-E2E 文档的修正

- 启动命令统一写 `bun run tauri`；它通过 `scripts/tauri-dev.mjs` 在 `apps/desktop` 工作目录启动 Tauri + Vite。不要在验收记录中继续写 `bun tauri dev`。
- `ChatDock`/伴侣面板是与 native player 并排的 layout sibling，不是覆盖 HWND 的“浮层”；“浮层”只可用于 mpv 自己的 OSC 控件语义，不能用于 AI、章节或文稿面板。
- AI 观剧流和自由聊天共享容器但不是同一数据投影；章节任务卡不能写进自由聊天 turns。
- 章节无容器章节时只能由用户点击「开始 AI 分段」触发；不要把旧的 soft/机械分段文案或自动启动 Agent 当作验收成功。

## 覆盖结论

- 自动化：fixture 生成、内嵌字幕存在性、无容器章节、时长下限和 ffprobe JSON 校验。
- 人工真实 Tauri：native HWND、用户点击、真实 Agent/ACP、MCP 字幕窗口与截帧、权限、重启和普通聊天隔离。
- 当前未覆盖：本工作树尚未具备可执行的 SQLite migration/diagnostic query、章节专用 ACP session、校验重试状态机和观剧流持久投影；这些必须在对应 L2 批次实现后重新执行本清单，不能以当前本地 evidence worker 的成功日志代替 F-4 全链路通过。
