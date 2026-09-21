# ACP / MCP 真实联调与手动验收

本文把两类验收分开：

- **自动真实联调**：`scripts/test-acp-mcp-tool-priority.mjs` 会真实启动 Lumina MCP 与 Codex ACP，依赖本机登录、Agent、网络和构建产物；不属于默认 unit/UI 测试。
- **人工桌面验收**：必须在 `bun run tauri` 的真实 Tauri 窗口执行。`bun run test:ui` 只有 mock 数据和 DOM 渲染覆盖，不能证明 Agent、MCP、libmpv 或 HWND 链路可用。

工作区布局与全屏/HWND 检查见 [`manual-e2e-workspace.md`](./manual-e2e-workspace.md)。

## 自动真实 ACP/MCP 联调

### 环境前置

- [ ] 已构建 `target/debug/lumina-app.exe`，且它支持 `--lumina-mcp`
- [ ] `node_modules/@agentclientprotocol/codex-acp/dist/index.js` 存在
- [ ] 本机 Codex 可执行文件存在；自定义路径通过 `CODEX_PATH` 指定
- [ ] Codex 已登录，且当前账户/模型允许完成一次真实回答
- [ ] 网络可用；该检查会产生真实 Agent 请求，默认不会运行

运行：

```powershell
$env:LUMINA_MCP_E2E="1"
bun run test:e2e:acp-mcp-priority
```

可选变量：

- `LUMINA_MCP_E2E_TIMEOUT_MS`：每个 RPC 的超时，默认 `90000`。
- `CODEX_PATH`：Codex 可执行文件路径。
- `LUMINA_MCP_COMMAND`：带 `--lumina-mcp` 的 Lumina 可执行文件路径。

帮助：`node scripts/test-acp-mcp-tool-priority.mjs --help`。

脚本使用临时脱敏 fixture snapshot；输出只允许出现检查阶段、可操作诊断和工具名，不输出 snapshot 原文、媒体绝对路径或 Agent stderr。

### 自动检查点

- [ ] MCP `initialize` 成功，`instructions` 含完整的 Lumina context 工具目录和 `tools/list` 规则
- [ ] `tools/list` 返回 `lumina_get_library_context` 与 `lumina_get_transcript_window`，且没有 web-search 工具
- [ ] `tools/call(lumina_get_playback_context)` 能读到当前媒体锚点/当前集，证明 snapshot 已被 MCP 读取
- [ ] `tools/call(lumina_get_transcript_window)` 能从 snapshot 的在线 transcript 返回台词
- [ ] ACP `session/new` 确实带 Lumina MCP server 配置
- [ ] 剧情问题产生 Lumina 工具事件；首个 Lumina 工具必须是 `lumina_get_library_context` 或 `lumina_get_transcript_window`
- [ ] 剧情问题的工具事件中没有 web search

### 失败诊断

脚本会打印 `code`、`phase` 和下一步 `action`。按阶段处理：

| 阶段 / code | 可操作处理 |
|---|---|
| `timeout` | 先确认 Agent/MCP 进程仍能启动；慢机器再提高 `LUMINA_MCP_E2E_TIMEOUT_MS`。不要把超时直接当成工具优先级失败。 |
| `process` | 检查 `CODEX_PATH`、`LUMINA_MCP_COMMAND`、`codex-acp` 文件和本机构建产物。 |
| `codex_state` | 确认 `CODEX_HOME` 可写，关闭其他 Codex 客户端释放 state runtime，再重跑；不要把它误判为 MCP 工具失败。 |
| `auth` | 在本机 Codex 客户端完成登录，再重新设置 `LUMINA_MCP_E2E=1` 运行。 |
| `mcp_instructions` / `mcp_tools_list` | 检查 MCP 版本、`LUMINA_MCP_TOOL_PROFILE=chat` 和 snapshot 能力字段。 |
| `snapshot` / `snapshot_transcript` | 确认 ACP prompt 前已经写入当前媒体 snapshot，且在线字幕已缓存；本地文件则准备真实可读媒体与字幕。 |
| `session_new` | 检查 ACP workspace 可写、MCP 命令可启动，以及 MCP server 配置是否被 Agent 接受。 |
| `no_lumina_tool` / `tool_priority` | 检查会话是否真的挂载 MCP、模型是否允许工具调用，以及稳定 instructions 是否仍要求剧情问题先读台词/媒体库。 |
| `web_search` | 保留工具名序列，检查 Agent profile/tool policy；剧情问题不得先走网络搜索。 |

自动脚本默认输出 `[skip]` 是预期行为，不等于真实联调通过。

## 人工 ACP 会话验收

### 前置

- [ ] 用 `bun run tauri` 启动真实桌面应用，并打开一个可播放媒体
- [ ] 顶部状态栏打开 **AI 对话**；Agent 设置中的 Codex 或 custom profile 显示可用
- [ ] 若验证在线媒体，先确认在线字幕已在文稿面板选择/缓存；在线 URL 不作为 ACP workspace cwd
- [ ] 打开日志目录入口，知道如何在失败时收集脱敏日志；UI 不应展示 stderr、路径或 JSON-RPC 原文

### 会话、流式和工具轨迹

- [ ] AI 对话显示为**右侧 sibling 工作区**，不覆盖视频像素；打开/收起不卸载正在运行的会话
- [ ] 发送“这一集主要讲了什么？”，流式中助手内容、思考/工具活动和最终答案均可见；完成后按“思考展示”设置收起或保留轨迹
- [ ] 若启用视图截图且模型具备视觉能力，画面问题才出现截图工具；关闭后新对话的 MCP 不再暴露截图工具
- [ ] 剧情/台词问题的工具活动优先显示 Lumina transcript/library 工具；不出现 web search
- [ ] 工具活动完成后，最终答案只显示业务内容，不把工具名、stderr、绝对路径或 `details` 当作用户文案

### 权限

- [ ] 权限模式为“每次询问”：工具调用出现审批条；批准后本轮继续，拒绝后给出业务化失败提示
- [ ] 切换为 Agent 默认/自动模式：不因普通只读 Lumina 工具调用反复弹审批；确需用户决策的写类动作仍按产品策略处理
- [ ] Agent 正在回答时尝试切历史或切 profile：操作被阻止或明确提示稍后重试，不静默丢回合

### 历史与结束会话

- [ ] 点击“历史对话”后列表来自 Agent session/list；刷新、关闭历史列表和回到当前对话都正常
- [ ] 选择同 profile、同工作目录的历史记录，能恢复 Agent 记忆并载入文本；切换媒体/profile 后不误恢复其他范围的线程
- [ ] 结束会话后当前 sessionId 提示消失；再次提问可新建会话
- [ ] 关闭右侧 AI 工作区再打开，进行中的面板状态仍在；Esc 只关闭工作区，不退出全屏（若 AI 已关闭则 Esc 才退出全屏）

### Agent 切换

- [ ] 空闲时在 Agent 设置切换 Codex / Antigravity / custom（按本机已配置项）；面板对话、标题和工具轨迹同步清空，不闪现旧 profile 内容
- [ ] 切换后新问题使用新 profile；旧 profile 的模型/推理设置不会泄漏到新 profile
- [ ] 未配置 Agent 时显示业务化“未配置/无法连接”提示，播放、字幕、笔记仍可用

## 记录

| 日期 | 构建/Agent | 自动脚本 | 人工结果 | 备注/诊断 code |
|---|---|---|---|---|
|  |  |  |  |  |

## 明确未覆盖

- DOM mock UI 测试不覆盖真实 ACP/MCP、登录、模型回答、网络、libmpv 或 HWND。
- 自动脚本不证明所有 UI 工作区布局、全屏交互、权限按钮视觉状态或历史列表可用性；这些必须按上面的人工清单确认。
- 当前迁移分支已接入 **AI 观剧流 / 自由聊天** 双 Tab，但仍必须在真实 Tauri 窗口中逐项验证；若实际构建入口缺失，应记录为未覆盖，不得用单一 AI 对话 dock 或 mock 测试代替通过。
