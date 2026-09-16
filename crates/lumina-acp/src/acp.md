# `lumina-acp`：Lumina 如何连接 AI Agent

这篇文档只回答一个问题：

> 用户在 Lumina 里发送一句话之后，Lumina 是怎样把这句话交给 AI Agent，
> 再把 Agent 的回答显示回来的？

如果你只想快速使用，先看第 1、2、4、8 节；如果你想读懂实现，再继续看后面的源码说明。

---

## 1. 先记住这件事：Lumina 是 ACP Client

`lumina-acp` 是 Lumina 内部使用的 **ACP Client**。它不直接调用 OpenAI、Anthropic
或其他模型 API，而是连接一个单独运行的 Agent 进程。

```text
Lumina UI
   │
   ▼
Tauri command / app adapter
   │
   ▼
lumina-acp（ACP Client）
   │  stdin/stdout：一行一条 JSON-RPC 消息
   ▼
Agent 进程（ACP 的另一端）
   │
   ▼
模型、工具和 Agent 自己的运行时
```

这里有三个角色：

- **Lumina**：启动 Agent、发送 prompt、接收事件、处理权限请求；
- **Agent**：真正调用模型、执行工具、维护对话 session，并通过 ACP 返回结果；
- **模型服务**：由 Agent 自己决定，例如 Responses API、本地模型或其他 provider。

在本地 stdio 场景中，Agent 通常是 Lumina 启动的子进程，所以它也可以理解成
“ACP 服务端一侧的进程”。Lumina 不需要自己实现一个 HTTP ACP 服务。

`AcpService::new()` 只创建一个空服务，不会启动进程：

```rust
use lumina_acp::AcpService;

let acp = AcpService::new();
```

没有配置 Agent 时，调用会返回 `NotConfigured`。这不会影响播放、字幕和笔记功能，
因为 ACP 在 Lumina 中是可选能力。

---

## 2. 为什么 Lumina 不绑定某一个 Agent？

ACP 的价值就是把“应用如何连接 Agent”这件事标准化。Lumina 只需要知道：

1. 用什么命令启动 Agent；
2. 如何通过 stdio 发送 ACP JSON-RPC；
3. 如何处理 `session/prompt`、`session/update` 和权限请求。

至于 Agent 内部使用哪个模型、怎样执行工具、怎样管理上下文，由 Agent 自己负责。

### 两种 ACP Agent

| 类型 | 含义 | 示例 |
| --- | --- | --- |
| 原生 ACP Agent | Agent 本身直接提供 ACP 命令 | Cursor CLI 的 `agent acp`、OpenCode 的 `opencode acp`、DeepSeek Harness 的 ACP 包 |
| ACP 适配器 | Agent 原本有自己的内部协议，由适配器把它包装成 ACP | Codex 的 `codex-acp`、Claude Code 的 `claude-agent-acp` |

对 `lumina-acp` 来说，两者的连接方式相同：都是一个命令、一些参数，然后通过
stdin/stdout 交换 ACP 消息。

例如，OpenCode 可以这样配置成一个 profile：

```rust
AgentProfile {
    id: "opencode".into(),
    name: "OpenCode".into(),
    kind: AgentKind::Custom,
    command: "opencode".into(),
    args: vec!["acp".into()],
    env: Default::default(),
}
```

Cursor CLI 的形式类似：

```rust
AgentProfile {
    id: "cursor".into(),
    name: "Cursor Agent".into(),
    kind: AgentKind::Custom,
    command: "agent".into(),
    args: vec!["acp".into()],
    env: Default::default(),
}
```

### “支持 ACP”还要满足什么？

“只要 Agent 支持 ACP 就能连接”需要加上范围限制：

- 当前 `lumina-acp` 支持的是**本地 stdio + 换行分隔 JSON-RPC**；
- Agent 至少需要支持 Lumina 使用的基础流程：`initialize`、`session/new`、
  `session/prompt` 和 `session/update`；
- Agent 的厂商私有扩展不一定被 Lumina 支持；基础聊天可以通用，但私有 UI 扩展、
  特殊计划面板等功能可能无法显示；
- Agent 的登录方式由 Agent 决定，Lumina 只负责执行 ACP 暴露出来的认证流程。

所以最准确的说法是：

> `lumina-acp` 不绑定 Codex、Claude 或任何特定模型。它可以连接任何提供 Lumina
> 当前支持的 stdio ACP 接口的 Agent。没有 ACP 的 Agent，需要先通过适配器接入。

相关项目：

- [ACP 协议](https://agentclientprotocol.com/)
- [Codex ACP adapter](https://github.com/agentclientprotocol/codex-acp)
- [Claude Agent ACP adapter](https://github.com/agentclientprotocol/claude-agent-acp)
- [Cursor CLI ACP](https://cursor.com/docs/cli/acp)
- [OpenCode ACP](https://opencode.ai/v2/docs/cli/acp/)
- [DeepSeek Harness ACP](https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/acp)

---

## 3. 一次普通聊天到底发生了什么？

应用最常调用的是：

```rust
acp.prompt(...)
```

第一次调用时，`AcpService` 还没有连接。内部会依次完成：

```text
prompt()
  │
  ├─ 1. 检查是否已有 LiveSession
  │
  ├─ 2. 没有的话，找到 Agent 启动命令
  │
  ├─ 3. 启动 Agent 子进程
  │
  ├─ 4. initialize：协商 ACP 能力
  │
  ├─ 5. authenticate：按 Agent 要求登录
  │
  ├─ 6. session/new 或 session/resume：创建/恢复会话
  │
  ├─ 7. session/prompt：发送用户问题
  │
  └─ 8. 读取 session/update，并不断通知 UI
```

第二次调用 `prompt` 时，通常会复用已有的 Agent 进程和 session。

这几个步骤分别对应源码中的位置：

| 步骤 | 代码 | 作用 |
| --- | --- | --- |
| 找到并启动 Agent | `agent/launch.rs`、`runtime/lifecycle.rs` | 解析命令、环境变量并启动子进程 |
| 构造 ACP JSON | `wire/session.rs`、`wire/codec.rs` | 生成 `initialize`、`session/new`、`session/prompt` 等消息 |
| 读写 stdin/stdout | `runtime/io.rs` | 把 JSON 写给 Agent，并等待对应响应 |
| 处理 Agent 事件 | `runtime/inbound.rs` | 把 `session/update` 转成 `AcpEvent` |
| 对外提供方法 | `runtime/service.rs`、`runtime/prompt.rs` | 管理 `AcpService` 和一轮 prompt |

---

## 4. 最小可用代码

下面是一个普通聊天的完整形状：

```rust
use lumina_acp::{
    AcpClientSettings, AcpEvent, AcpService, AgentProfilesHint, VideoPromptContext,
};

let acp = AcpService::new();
let profiles = AgentProfilesHint {
    active_profile_id: "codex".into(),
    profiles: vec![], // 空数组：使用内置 profile
};

let answer = acp.prompt(
    "这段视频主要讲了什么？",
    Some(r"D:\videos\demo.mp4".into()), // cwd hint，也可以传目录
    None,                                // None：使用 active profile
    Some(VideoPromptContext {
        media_path: Some(r"D:\videos\demo.mp4".into()),
        media_title: Some("demo.mp4".into()),
        position_ms: Some(45_000),
        ..Default::default()
    }),
    None, // 历史摘要，可选
    None, // 会话恢复信息，可选
    AcpClientSettings::default(),
    profiles,
    |event| match event {
        AcpEvent::AgentMessage { text } => println!("{text}"),
        AcpEvent::Finished { .. } => println!("完成"),
        AcpEvent::Failed { code, message } => eprintln!("{code}: {message}"),
        _ => {}
    },
)?;

println!("最终回答：{answer}");
```

如果希望用户打开聊天页面时就提前建立连接，可以先调用 `connect`：

```rust
acp.connect(
    None,
    None,
    None,
    AcpClientSettings::default(),
    profiles.clone(),
    |_| {},
)?;
```

`connect` 只负责预热 Agent 和 session，不发送用户问题。不调用它也没关系，第一次
`prompt` 会自动完成连接。

对话操作的区别如下：

| 方法 | 做什么 |
| --- | --- |
| `AcpService::new()` | 创建空服务，不启动进程 |
| `connect()` | 提前启动 Agent 和 session |
| `prompt()` | 发送一轮问题并接收流式事件 |
| `new_chat()` | 尽量保留进程，但创建一个新的 ACP session |
| `close_session()` | 关闭 session 并清理 Agent 进程 |
| `close_session_for_shutdown()` | 应用退出时快速清理进程 |

---

## 5. 发送给 Agent 的到底是什么？

ACP 使用 stdin/stdout 通信，每一行是一条 JSON-RPC 消息。`wire/` 目录负责构造和
解析 JSON，`runtime/io.rs` 负责实际读写管道。

发送的消息大致如下：

```json
{"id":1,"method":"initialize","params":{"protocolVersion":1}}
{"id":2,"method":"session/new","params":{"cwd":"D:/videos","mcpServers":[]}}
{"id":3,"method":"session/prompt","params":{"sessionId":"sess_1","prompt":[{"type":"text","text":"这段视频主要讲了什么？"}]}}
```

Agent 会在处理过程中发来通知：

```json
{"method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":"这是回答的一部分"}}}
```

`runtime/inbound.rs` 会把这些消息转换成应用能理解的事件：

- 文本增量 → `AcpEvent::AgentMessage`；
- 思考增量 → `AcpEvent::AgentThought`；
- 工具调用 → `AcpEvent::ToolCall`；
- 计划更新 → `AcpEvent::Plan`；
- 权限请求 → `AcpEvent::PermissionRequest`。

因此 React 和 Tauri command 不需要直接理解 ACP JSON，也不需要接触子进程的
stdin/stdout。

---

## 6. 视频上下文怎样传给 Agent？

`VideoPromptContext` 是一个普通数据结构：

```rust
pub struct VideoPromptContext {
    pub media_path: Option<String>,
    pub media_title: Option<String>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub subtitle_choice_id: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub episode_title: Option<String>,
    pub episode_overview: Option<String>,
}
```

它不是完整的系统提示词，也不是每个字段都会直接拼进本轮 prompt。

当前 `wire/session.rs::session_prompt_params` 的规则是：

- 本地媒体路径转换为 `resource_link`；
- 在线页面 URL 保留为 URL；
- `history_context` 作为可选的历史摘要；
- 用户问题作为本轮最后一段文本；
- 播放进度、集数和字幕轨道作为轻量动态上下文每轮发送；
- 当前集标题/剧情只在媒体或集数切换时发送；
- 章节和笔记不进入每轮 ACP prompt，笔记也不默认暴露给 ACP/MCP。

可以把它理解成三部分：

```text
本轮 prompt = 媒体链接 + 播放进度/集数/字幕 + 换集时的本集信息 + 历史摘要（可选）+ 用户问题
snapshot    = 当前播放锚点、当前集信息、series 预热缓存与在线媒体信息
MCP         = Agent 需要时读取可用 snapshot 数据的工具
```

章节和笔记的按需通道不是本 crate 的能力；在线章节可随在线媒体 snapshot 返回，
本地 ffprobe 章节与用户笔记当前不默认进入 ACP/MCP。未来新增本地章节通道时，
应在独立计划中确定统一载体。

`lumina-acp` 不直接实现 snapshot 或 MCP。宿主应用通过 `SessionEnvironment` 提供
这些能力：

```rust
lumina_acp::set_default_environment(std::sync::Arc::new(AppSessionEnvironment));
```

---

## 7. Agent profile 是什么？

profile 就是一份“怎样启动某个 Agent”的配置：

```rust
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}
```

启动前，`agent/launch.rs::resolve_launch` 会把 profile 解析成完整的启动信息：

- 最终程序路径；
- 命令参数；
- 需要注入的环境变量。

Codex 默认 profile 会尝试寻找：

1. 已安装的 `codex-acp` 单文件；
2. 开发环境中的 ACP 入口文件；
3. Windows 上的 `bun x @agentclientprotocol/codex-acp`；
4. `bunx @agentclientprotocol/codex-acp`。

这些是 Codex 的启动便利逻辑。自定义 Agent 不需要修改 Rust 代码，只需要提供自己的
`command`、`args` 和 `env`。

---

## 8. 普通聊天和隔离任务

这两种任务都使用 ACP，但目的不同。

### 普通聊天

普通聊天使用长期存在的 `AcpService`：

- 可以复用已有 session；
- 可以传递历史摘要；
- 可以连接 Lumina MCP；
- 可以向 UI 发送流式事件。

### 一次性隔离任务

翻译、校对和媒体信息解析等短任务使用：

```rust
let result = AcpService::prompt_isolated_restricted(
    task_prompt,
    "codex".into(),
    profiles,
    model_selection,
    Some("library:resolver".into()), // 只用于日志，不发送给模型
)?;
```

这个方法会创建一个新的 service 和 session，任务结束后关闭它。它不会使用聊天历史、
视频上下文或 Lumina MCP 工具。没有有效文本时返回 `AcpErrorCode::NoOutput`。

### 批量隔离任务：`WorkshopPool`

字幕翻译通常需要同时处理多个批次，因此使用作业级 session pool：

```rust
use std::sync::Arc;
use lumina_acp::{PoolConfig, WorkshopPool};

let pool = Arc::new(WorkshopPool::new(
    PoolConfig::new("codex", profiles),
    "subtitle-job-123".into(),
));

let text = pool.submit(prompt, Some("batch=1".into()), None)?;
pool.shutdown();
```

默认有 4 个槽位。每个槽位拥有自己的 `AcpService` 和 ACP session：

- 一个 batch 会占用一个槽位；
- batch 结束后释放槽位，但 session 可以被后续 batch 复用；
- `SpawnFailed` 或 `ProtocolError` 最多在同一个槽位重试一次；
- 作业结束时调用 `shutdown`，清理 Agent 和相关 rollout 文件；
- `submit` 是阻塞调用，应放到 `spawn_blocking` 或其他阻塞线程中。

`IsolatedSessionPool` 是 `WorkshopPool` 的兼容别名。

---

## 9. 权限、取消和错误

### 权限请求

Agent 需要执行文件或终端操作时，可能向 Client 请求权限。

- `PermissionMode::Auto`：自动选择允许选项；
- `PermissionMode::Ask`：发送 `AcpEvent::PermissionRequest`，等待 UI 选择。

UI 处理事件后调用：

```rust
// 用户允许：
acp.respond_permission(&request_id, Some(option_id))?;

// 用户拒绝：
acp.respond_permission(&request_id, None)?;
```

### 取消请求

```rust
acp.request_cancel();
```

这会先发送 `session/cancel`。如果 Agent 在规定时间内没有结束，ACP 会终止子进程，
避免留下卡死的后台任务。

### 错误返回

错误统一具有以下结构：

```text
{ code, message, details? }
```

UI 只显示 `message`。`details` 只用于日志和排查底层问题，例如 stderr、JSON 解析
失败或具体路径。

常见错误：

| 错误 | 含义 |
| --- | --- |
| `NotConfigured` | 没有可用的 Agent，或 Agent 尚未登录 |
| `Busy` | 同一个 `AcpService` 正在执行另一轮 prompt |
| `SpawnFailed` | Agent 进程启动失败 |
| `ProtocolError` | Agent 返回了无法识别的 ACP 消息 |
| `NoOutput` | 隔离任务没有得到有效文本 |
| `Cancelled` | 用户或超时取消了本轮请求 |

---

## 10. 源码应该怎样阅读？

```text
lib.rs
  根导出和旧模块路径兼容层

domain/
  VideoPromptContext、AcpEvent、设置和其他数据结构

agent/
  查找 Agent、解析 profile、准备启动命令

wire/
  构造和解析 ACP JSON

runtime/
  持有真实子进程，负责 session 生命周期、读写和事件分发

jobs/
  一次性任务、WorkshopPool、rollout 清理和回答收集
```

推荐阅读顺序：

1. `runtime/service.rs`：看 `AcpService` 对外提供什么；
2. `runtime/prompt.rs`：看一轮 `prompt` 怎样运行；
3. `runtime/lifecycle.rs`：看 Agent 怎样启动和建立 session；
4. `wire/session.rs`：看实际发送了哪些 ACP JSON；
5. `agent/launch.rs`：看 Codex 或自定义 Agent 怎样被找到；
6. `jobs/pool.rs`：看批量任务怎样复用 session。

如果只是接入普通聊天，通常只需要根导出的：

```text
AcpService
AcpClientSettings
AgentProfilesHint
VideoPromptContext
AcpEvent
```

其余模块主要是 ACP Client 的内部实现。
