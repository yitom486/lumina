# ACP 连接原理与方法手册

本文档讲透 `lumina-acp`：进程怎么起来、会话怎么连、每个公开方法是什么含义、
最小可用代码怎么写。模块分层总览见 [`README.md`](./README.md) §4。

> 约定：本 crate 只做 **ACP Client**（stdio JSON-RPC），是可选能力。
> 未配置 Agent 时返回 `NotConfigured`，播放 / 字幕 / 笔记不受任何影响。

---

## 1. 生命周期总览

`AcpService` 最多持有一个 `LiveSession`（子进程 + `sessionId`）：

```text
无 session
  │ connect()            # spawn 进程 → initialize → authenticate → session/new|resume
  ▼
有 session（常驻复用）
  │ prompt() *           # 复用 session 发 session/prompt，流式收 session/update
  │ new_chat()           # 不杀进程，只换 session（session/close + session/new）
  │ set_session_model()  # 不换 session，只改 model/reasoning 配置
  │ request_cancel()     # 任意时刻：发 session/cancel，8 秒不响应则强杀
  ▼
close_session() / close_session_for_shutdown()   # session/close + 杀进程树
```

两类用法共享同一套基础设施，但生命周期相反：

| 用法 | 生命周期 | 历史/工具 | 入口 |
|---|---|---|---|
| Chat（多轮） | 长连接，常驻复用 | 有历史，有 MCP 工具 | `connect / prompt / new_chat` |
| Isolated（翻译/解析等短任务） | 一次性，用完即关 | 无历史，无工具 | `prompt_isolated_restricted` 或 `WorkshopPool` |

一个 `AcpService` 同时只跑一个 prompt（原子 `busy` 锁，撞上返回 `Busy` 中文错）。
需要并发批量任务时，用 `jobs::pool::WorkshopPool`（默认 4 槽，每槽一个独立
`AcpService` + session，动态抢空闲槽）。

---

## 2. 连接原理

### 2.1 传输层：stdio 上的行分隔 JSON-RPC 2.0

- 子进程的 `stdin` 写请求（每行一个 JSON），`stdout` 按行读响应/通知，`stderr`
  单独线程排空，只进 `tracing` 日志，永不阻塞 Agent。
- Client → Agent：`initialize`、`authenticate`、`session/new`、`session/resume`、
  `session/prompt`、`session/cancel`、`session/close`、`session/set_config_option`。
- Agent → Client（反向请求，由 `runtime::host::AcpHost` 应答）：
  `session/request_permission`、`fs/read_text_file`、`fs/write_text_file`、
  `terminal/create|output|wait_for_exit|kill|release`。
- 读写按 `id` 配对等待（`runtime/io.rs`）；读写之间穿插的 `session/update`
  通知会被即时转成 `AcpEvent` 回调，不会吞掉（`runtime/inbound.rs`）。
- 超时（写死在代码里并有单测锁定）：`initialize` 180s（Windows）/ 60s（Unix）、
  `session/*` 60s、`authenticate` 120s、整轮 prompt 上限 600s、取消宽限 8s、
  权限等待 120s。

### 2.2 进程怎么起来的（`runtime/lifecycle.rs::spawn_session`）

1. **定 `cwd`**（`agent/workspace.rs::resolve_session_cwd`）：显式 hint（目录或媒体
   文件的父目录）→ 相对路径拼进程 cwd → 都没有则建默认 workspace。
   在线 URL **拒绝**做 cwd，回退受保护的 app-data workspace。
2. **定启动命令**（`agent/launch.rs::resolve_launch`，Codex 默认 profile 按序命中
   第一个可用项）：
   1. 单文件 `codex-acp(.exe)`（`native/acp/`、程序目录、`PATH`）——发布包带它就零依赖；
   2. 开发树 `node_modules/@agentclientprotocol/codex-acp/dist/index.js` + `bun run`；
   3. Windows 下 `bun x @agentclientprotocol/codex-acp`（stdio 管道比 `bunx` 稳）；
   4. 兜底 `bunx @agentclientprotocol/codex-acp`（首次按需下载适配器）。
   
   同时补环境：`CODEX_PATH`、`CODEX_HOME`、`TERM`，bun/npm 的 bin 目录并入 `PATH`。
   找不到可执行文件 → `NotConfigured`（“未配置 AI Agent（可选）”）。
3. **`Command::spawn`**（Windows 带 `CREATE_NO_WINDOW`，不闪控制台）。
4. **握手**：
   - `initialize`（声明 `fs` + `terminal` 能力；隔离任务声明空能力，对方就不能
     把媒体目录当工具 workspace）→ 解析 `protocolVersion`、`agentCapabilities`
    （是否支持 close/resume）、`authMethods`；
   - `authenticate`（仅当 Agent 广告了方法）：`~/.codex` 存在优先 `chat-gpt`，
     否则有 `OPENAI_API_KEY` 优先 `api-key`。Codex 认证失败 → 中文
     “Codex 尚未登录…请运行 `codex login`”；
   - `session/new`（或条件满足时 `session/resume`），带上 `cwd` + 宿主给的
     `mcpServers` 配置。`sessionId` 拿不到 → `ProtocolError` 并杀树，不留僵尸。
5. 成功后 `LiveSession` 常驻 `AcpService`，`Drop` 默认杀整棵进程树
  （Windows 用 `taskkill /PID /T /F`，因为 Codex 常起第二进程，只杀 wrapper 会泄漏）。

### 2.3 prompt 原理（`runtime/prompt.rs::run_prompt_inner`）

1. 无 live session 则先走一遍 §2.2（并应用一次性 model 选择）。
2. 组装 `session/prompt`（`wire/session.rs::session_prompt_params`）：只放
   **媒体 `resource_link`** + 历史摘要 + 用户文本。播放锚点/章节/笔记正文
   **不**进 prompt，写在 snapshot 里由 MCP 工具按需读。
3. 循环读 stdout：`agent_message_chunk` → `AgentMessage`，`agent_thought_chunk` →
   `AgentThought`，`tool_call(*)` → `ToolCall(*)`，`plan` → `Plan`。
   `AgentReplyCollector` 只保留工具调用之后**最后一段**正文（Agent 常在工具前
   说客套话）。
4. 读到对应该 `prompt_id` 的 response 即收尾：正常 → `Finished{text, stop_reason}`；
   空文本聊天返回中文提示（原有 UX），隔离任务返回类型化 `NoOutput` 错误
   （防止把提示语误解析成 JSON 结果）。
5. 失败（传输错/超时/空输出）→ 丢弃 live session，下次重建；取消
   （`request_cancel` 或超时）→ `Cancelled`，取消宽限 8s 内不退出则杀树。

### 2.4 反向通道与权限（`runtime/host/` + `runtime/inbound.rs`）

- 隔离任务（`tool_access_enabled=false`）收到任何 Agent→Client 工具请求直接回
  `Tool access is disabled`，连 `AcpHost` 都不进。
- `PermissionMode::Auto`：自动选 `allow_*` 选项，否则取消；
  `PermissionMode::Ask`：经 `AcpEvent::PermissionRequest` 把选项抛给 UI，
  UI 调 `respond_permission(request_id, option_id)` 回填（120s 不答按拒绝）。
- `fs/*` 相对路径相对 session workspace 解析；`terminal/*` 由 `AcpHost` 托管
  子进程（输出上限截断、600s 等待上限），session 结束或应用退出统一释放。

---

## 3. 方法参考（`AcpService`）

> 全部无 `unwrap` / `expect`；错误统一 `{ code, message, details? }`，
> UI 只展示 `message`（见 [`error.rs`](./error.rs)）。

| 方法 | 签名要点 | 含义 |
|---|---|---|
| `new` | `() -> Self` | 建空服务，不启动任何进程 |
| `status` | `(&profiles) -> AcpStatus` | 是否可用、当前 profile、各 profile 可用性、`busy`、`session_active`、live session 的模型选项；纯查询，无副作用 |
| `is_busy` | `() -> bool` | 是否有 prompt 在跑 |
| `connect` | `(cwd?, profile_id?, saved_session?, settings, profiles, on_event)` | 预热：无 session 则建连（§2.2），有则只做 snapshot 能力同步 + 应用 model 选择；不发用户问题 |
| `prompt` | `(text, cwd?, profile_id?, context?, history?, saved_session?, settings, profiles, on_event) -> String` | 聊天主入口：复用/建连后发一轮 `session/prompt`，返回最终正文；流式经 `on_event` |
| `new_chat` | `(cwd?, profile_id?, settings, profiles, on_event)` | 新对话：不断进程，`session/close` + `session/new`（对方不支持 close 则重连） |
| `set_session_model` | `(model_id?, reasoning_effort?, on_event) -> AcpSessionModelOptions` | 在线换模型/推理强度，不旋转 session |
| `respond_permission` | `(request_id, option_id?)` | `Ask` 模式下回填用户对权限请求的选择 |
| `request_cancel` | `()` | 软取消：置位 + 发 `session/cancel`（超时强杀逻辑在 prompt 循环里） |
| `close_session` | `() -> Result<_, AcpError>` | 结束 session 并清理进程（等子进程回收） |
| `close_session_for_shutdown` | `()` | 应用退出用：不等待握手，直接杀树 |
| `prompt_isolated_restricted` | 关联函数 `(text, profile_id, profiles, model_selection?, task_label?) -> String` | 一次性隔离任务（§1 表）：新进程、无历史、无工具，结束即关；薄转发，实装在 `jobs::isolated` |
| `discover_isolated_models` | 关联函数 `(profile_id, profiles) -> AcpModelDiscoveryResult` | 短连接只取 Agent 的可选模型/推理强度，不发任何用户或媒体数据 |

### `jobs::pool::{WorkshopPool, PoolConfig}`（批量隔离任务）

```rust
let pool = WorkshopPool::new(PoolConfig::new("codex", profiles), "job-123".into());
// PoolConfig 字段：size（默认 DEFAULT_POOL_SIZE = 4）、profile_id、profiles、model_selection
let text = pool.submit(prompt, task_label, retry_label)?; // 阻塞调用，放 spawn_blocking
pool.shutdown();                                          // 作业结束显式关闭（含本任务 rollout 清理）
```

- 槽位抢占复用：一个完整 batch（含内容重试）独占一槽，batch 结束释放，
  session 留给下一 batch；传输失败（`SpawnFailed`/`ProtocolError`）同槽重试一次。
- `open_conversation()` 可拿 `AgentConversation`（`lumina-core` port）做多轮
  batch；`Drop for WorkshopPool` 会自动 `shutdown`。
- `IsolatedSessionPool` 是 `WorkshopPool` 的通用别名，旧名保留。

### 事件与错误速查

`AcpEvent`：`Started` / `Progress{message}` / `AgentMessage{text}` /
`AgentThought{text}` / `ToolCall{…}` / `ToolCallUpdate{…}` / `Plan{text}` /
`SessionSaved{session_id, profile_id, cwd}` / `PermissionRequest{…}` /
`PermissionResolved{…}` / `Finished{text, stop_reason}` / `Failed{code, message}`。

`AcpErrorCode`：`NotConfigured`（未配置/未登录，可选能力）/ `Busy` /
`WorkspaceUnavailable` / `SpawnFailed` / `ProtocolError` / `NoOutput` /
`Cancelled` / `InternalError`。用户输入校验用中文具体文案（如“提问内容不能为空”）。

---

## 4. 最小案例

### 4.1 聊天：连接 → 提问 → 关闭

```rust
use lumina_acp::{
    AcpClientSettings, AcpEvent, AcpService, AgentProfilesHint, VideoPromptContext,
};

let profiles = AgentProfilesHint {
    active_profile_id: "codex".into(),
    profiles: vec![], // 空即用内置默认 profile
};
let acp = AcpService::new();

// 可选：先看状态
let status = acp.status(&profiles);
assert!(status.available); // 否则看 status.message / status.hint 按指引安装

// ① 预热（用户打开聊天页时调，不发问题）
acp.connect(
    None, None, None,
    AcpClientSettings::default(),
    profiles.clone(),
    |ev| println!("{ev:?}"),
)?;

// ② 提问（复用 session，流式回调 + 最终返回全文）
let answer = acp.prompt(
    "解释一下刚才提到的内容。",
    None,                       // cwd：传媒体所在目录，或 None 走默认规则
    None,                       // profile_id：None 用 active
    Some(VideoPromptContext {
        media_path: Some(r"D:\videos\demo.mp4".into()),
        media_title: Some("demo.mp4".into()),
        position_ms: Some(45_000),
        ..Default::default()
    }),
    None,                       // 历史摘要：需要多轮记忆时传稳定摘要
    None,                       // 会话恢复：恢复上次会话时传 SavedSessionHint
    AcpClientSettings::default(),
    profiles.clone(),
    |ev| match ev {
        AcpEvent::AgentThought { text } => println!("[思考] {text}"),
        AcpEvent::AgentMessage { text } => println!("[消息] {text}"),
        AcpEvent::Finished { .. } => println!("--- 回答完毕 ---"),
        AcpEvent::Failed { code, message } => eprintln!("{code}: {message}"),
        _ => {}
    },
)?;

// ③ 新对话 / ④ 关闭
acp.new_chat(None, None, AcpClientSettings::default(), profiles.clone(), |_| {})?;
acp.close_session()?;
```

### 4.2 一次性隔离任务（一行）

```rust
let result = AcpService::prompt_isolated_restricted(
    task_prompt,
    "codex".into(),
    profiles,
    model_selection,          // Option<AcpSessionModelSelection>，None 用 Agent 默认
    Some("library:resolver".into()), // 仅日志追踪，不发给模型
)?; // 无有效文本 → Err(NoOutput)，不要当正文解析
```

### 4.3 取消与权限（UI 侧）

```rust
// 用户点“停止”：
acp.request_cancel();

// PermissionMode::Ask 时，收到 PermissionRequest 事件后弹确认框，
// 用户确认则：
acp.respond_permission(&request_id, Some(option_id))?;
// 用户拒绝 / 超时则传 None（按拒绝处理）。
```

### 4.4 宿主接线（app 侧，一次）

```rust
// 应用启动时安装一次：snapshot 路径、能力同步、Chat/isolated 的 MCP 配置
lumina_acp::set_default_environment(std::sync::Arc::new(AppSessionEnvironment));
```

`SessionEnvironment` 的四个方法（`snapshot_path` / `sync_snapshot` /
`mcp_servers` / `snapshot_vision_capable`）是 `lumina-acp` 不触碰 MCP/媒体库
实现的边界：快照读写与 MCP 配置永远由宿主提供（见
[`domain/environment.rs`](./domain/environment.rs)）。
