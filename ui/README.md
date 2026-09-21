# Lumina UI 设计参考与交互约定

这里保存 Lumina 的 UI 视觉参考图和与实现相关的交互约定。图片用于统一信息层级、颜色、间距和面板关系；实际实现仍必须遵守项目的 HWND / React / Tauri 架构边界。

## 视觉参考

- `lumina-desktop-overview-reference.png`：主播放工作区与 AI 上下文面板。
- `lumina-transcript-reading-reference.png`：文稿跟随播放、引用和询问 AI。
- `lumina-notes-workspace-reference.png`：时间戳笔记、字幕引用和复习卡片入口。
- `lumina-settings-workspace-reference.png`：AI Agent 设置与字幕任务设置。
- `lumina-general-chat-rich-output-reference.png`：通用聊天和富组件回答。
- `lumina-ai-watch-feed-reference.png`：AI 观剧流与自由聊天 Tab、章节独立任务卡和信息流。
- `lumina-chapter-generation-reference.png`：无容器章节时的 AI 分段工作区。
- `lumina-ai-automation-settings-reference.png`：快捷提问、富输出和章节策略设置。

## 通用聊天

- 通用聊天入口是顶部栏和左侧导航中的「AI 对话」，对应现有 `ChatDock`，属于独立布局列，不覆盖原生播放器。
- AI Dock 与选集、文稿、笔记、章节等普通页面共享同一个右侧 `WorkspacePanelFrame` 外壳和工作区槽位；统一的是宽度、边界、标题栏与切换规则，不是数据或会话。
- AI 面板通过 `AI观剧流` / `自由聊天` Tab 切换；两者共享视觉容器，但只有「自由聊天」承载主聊天历史。AI 打开时普通 sidebar 暂时让出同一工作区槽位，关闭后恢复原来的 tab，禁止做成覆盖 HWND 的浮层。
- 章节分析显示为观剧流中的独立任务卡，使用独立 Agent session；它可以共享 profile、工作目录和
  Lumina 工具，但不得写入自由聊天的 turns、历史或上下文。
- 文稿、章节或笔记中的「询问 AI」属于上下文快捷入口：打开通用聊天、带入当前时间点和引用内容，并预填可编辑的提示词。
- 默认不自动发送。用户确认或修改后，再点击发送；这样可以避免误触发长任务、模型调用或敏感上下文发送。
- 「一键发送」可以作为设置项提供，但默认关闭；启用后仍应在输入框附近显示当前视频、章节和字幕上下文。

## 富文本与富组件输出

- `ChatMarkdown` 继续负责叙述性内容：标题、段落、列表、表格、代码块、GFM、数学公式和可信引用。
- AI 的交互内容不允许直接输出任意 HTML、React 代码或未经校验的组件名称。
- 结构化结果先归一化为受限的 `AssistantBlock`，再由 `RichBlockRenderer` 按白名单渲染。
- 版本化快捷任务的本地 `ChatTurn` 必须保留任务 ID；`chapter_recap.v1`、
  `chapter_outlook.v1`、`plot_summary.v1` 和 `question_candidates.v1` 只能通过任务专属的
  白名单适配器进入 `AssistantBlock`，未知或损坏结果显示业务兜底，不把原始 JSON 交给渲染器。
- 第一批富组件建议包括：关键点提示、字幕引用、时间线、章节列表、保存为笔记、跳转播放、继续追问。
- 所有可操作组件必须携带来源时间或章节锚点；点击「跳转播放」只调用播放器 seek，不直接改变 HWND 生命周期。
- 流式输出时先渲染 Markdown 文本，结构化组件在 JSON block 完整后再显示，避免半截卡片闪烁。
- 业务富组件放在 `packages/chat-ui`；`packages/ui` 只提供 Radix/shadcn 通用原语，
  `apps/desktop` 负责 Tab、任务状态和真实数据接线。

## 章节生成策略

章节只允许两种来源：

1. 容器或在线源自带的真实章节。
2. 用户主动触发的 AI 语义分段：结合台词/字幕和按需截图逐步分析。

- 无真实章节时，章节面板显示提醒和「开始 AI 分段」按钮；打开视频不静默启动 Agent。
- AI 分段使用独立 Agent 会话，但共享当前 Agent profile、工作目录和 Lumina 工具集。
- 输出必须经过结构校验；校验失败时向同一会话增量追加错误报告，最多重试三次。
- 专有提示词按任务版本管理，快捷按钮只引用任务 ID，不在组件内散落 prompt 文本。

提示词、校验器、重试状态、章节版本、问题候选和截图索引由
`.plan/L2-chapter-agent-data-pipeline.md` 统一定义。

没有字幕时仍不自动启动 ASR；用户可以在可用条件下主动选择对应处理路径。

## 主题变量

- 业务组件只能使用语义 token，例如 `background`、`surface`、`accent-ai`、`accent-playback`、
  `border` 和 `muted-foreground`。
- 亮色与暗色必须各自提供完整 token 映射；渐变和透明色也应由变量或 `color-mix()` 派生，
  不得在页面组件里写固定色值。

## HWND 布局约束

- `VideoSurface` 占用的矩形就是 libmpv HWND 的唯一 bounds。
- 文稿、笔记、章节、AI 和设置使用 sibling layout，不使用覆盖 HWND 的透明 WebView 层。
- 设置页面进入时隐藏或缩小 HWND；退出设置后再通过 surface bounds 同步恢复播放器。
- 全屏顶部控制区必须通过预留布局空间放在 HWND 之外；不要把中心弹窗、向上展开菜单放到 `PlayerBar` 上方。
