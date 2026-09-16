# lumina-acp

`lumina-acp` 是 Lumina 的通用 Agent Client Protocol（ACP）客户端库。它通过
`stdio JSON-RPC 2.0` 与本机 Agent 子进程（如默认的 Codex ACP 适配器、Claude
Code 或自定义 Agent）通信，负责会话生命周期、流式事件、权限请求、取消和错误
边界。

它是可选能力：没有配置 Agent 时，播放、字幕和笔记功能仍然可以独立运行。

> 深入文档：进程启动顺序、握手与 prompt 原理、`AcpService` 全部方法含义、
> 最小可用案例，见 [`acp.md`](./acp.md)（连接原理与方法手册）。

---

## 1. 模块定位与职责

在 Lumina 架构中，`lumina-acp` 确立了应用与 AI 智能体之间的**稳定开放协议边界**：

```text
[React 前端 / Chat 对话面板]
        │ (Tauri Commands / Events)
        ▼
   [lumina-acp] (标准 ACP Client)
        │
        │ (跨平台 stdio JSON-RPC 管道)
        ▼
   [Agent Profile 子进程] (如 codex-acp / 自定义 Agent)
        │
        ▼
   [Codex App Server / 模型后端] (如 Responses API)

   [AppSessionEnvironment]
        ├─ session/new|resume 的 MCP 配置
        ├─ Lumina snapshot 路径与能力同步
        └─ Chat / isolated 任务工具权限隔离
```

- **职责**：
  - 维持纯客户端（Client-only）身份：绝不把特定供应商私有 API（如 Codex 私有协议）作为主协议，只面向标准通用的 ACP 编程。
  - 按需启动与子进程托管（Process Lifecycle）：用户发起交互时才 spawn 对应 Agent Profile；提供会话初始化（`initialize`）、认证（`authenticate`）和会话建立（`session/new`）。
  - 双向 RPC 调度：
    - **Client → Agent**：`session/prompt`、`session/cancel`、`session/set_config_option`。
    - **Agent → Client**：处理来自 Agent 的权限请求、终端/文件系统请求、工具进度和其他 session update。
  - 流式文本与思考过程解析（`agent_reply_collector`）：增量还原 Agent 的推理思考（Thinking / Reasoning 块）与最终回答。
  - 动态视频上下文打包（`context`）：每轮只传递媒体 `resource_link`；播放锚点、章节、笔记和媒体库状态写入 snapshot，由 MCP 工具按需读取。稳定工具说明由 MCP 初始化阶段提供，不重复注入每轮 prompt。
  - 会话环境抽象（`SessionEnvironment`）：`lumina-acp` 不直接依赖 Lumina 的 MCP、媒体库或快照实现，由宿主应用提供路径、能力和 MCP server 配置。
  - 普通聊天与隔离任务分离：聊天可以使用历史和 Lumina MCP；翻译/元数据解析等短任务使用无历史、无工具的受限 session。字幕翻译/校对由宿主按作业创建 `WorkshopPool`，复用隔离 session 槽位，作业结束后统一关闭。
- **硬约束**：
   - 未配置 Agent（如初次使用、无 API Key）时，必须返回 `NotConfigured`，播放/字幕/笔记功能**绝对不能因此受到任何影响**。
  - `message` 是稳定的业务提示；底层 stderr、serde、路径和协议细节只进入 `details` 与日志。
  - 严防终端僵尸进程：设置明确的超时与取消保护（`CANCEL_KILL_SECS`），Agent 超时不响应取消则强制终止。

源码按 `domain → agent → wire → runtime (+ jobs)` 分层；**各目录一句话职责与如何拼成一次会话**，见下方 **§4**。

---

## 2. 构建思路与设计原则

1. **协议层与 Agent 引擎解耦**：
   - 换模型 ≠ 换 Agent。模型/推理选项通过 session 配置调整；切换 Codex、Claude 或自定义 Agent，则由 profile 的启动命令和参数决定。
2. **权限模式由宿主应用控制**：
   - `PermissionMode::Auto` 自动处理可放行请求；`PermissionMode::Ask` 通过 UI 请求用户确认。隔离任务不开放 Lumina MCP 工具。
3. **环境隔离与工作空间自适应**：
   - 本地媒体通常使用媒体目录作为 `cwd`；在线 URL 不直接作为 Windows 工作目录，而是回退到受保护的 App-Data workspace。
4. **缓存友好的上下文边界**：
   - 固定工具说明由 MCP `initialize.result.instructions` 提供；每轮 prompt 只携带媒体链接、历史摘要和用户输入，动态播放数据放在 snapshot 中按需读取。

---

## 3. 对外使用指南

### 在 Cargo workspace 中添加依赖

```toml
[dependencies]
lumina-acp.workspace = true
```

### 代码使用示例

`AcpService::prompt` 的参数较多，是因为它同时接收工作目录、Profile、会话恢复、
客户端设置和事件回调。真实应用通常由 Tauri command / app adapter 组装这些参数。

#### 1. 查看 Agent 状态

```rust
use lumina_acp::{AcpService, AgentProfilesHint};

let acp = AcpService::new();
let profiles: AgentProfilesHint = load_profiles_from_app_settings();
let status = acp.status(&profiles);

println!("当前 Profile: {}", status.active_profile_id);
println!("Agent 可用: {}", status.available);
```

#### 2. 发起普通聊天并接收事件

```rust
use lumina_acp::{
    AcpClientSettings, AcpEvent, AcpService, AgentProfilesHint, VideoPromptContext,
};

let acp = AcpService::new();
let profiles: AgentProfilesHint = load_profiles_from_app_settings();

let context = VideoPromptContext {
    media_path: Some(r"D:\videos\demo.mp4".into()),
    media_title: Some("demo.mp4".into()),
    position_ms: Some(45_000),
    duration_ms: None,
    subtitle_choice_id: Some("embedded:0".into()),
    season: None,
    episode: None,
    episode_title: None,
    episode_overview: None,
};

let answer = acp.prompt(
    "解释一下刚才提到的内容。",
    Some(r"D:\videos\demo.mp4".into()),
    Some("codex".into()),
    Some(context),
    None, // history_context；需要时传入稳定摘要
    None, // saved_session；需要恢复会话时传入
    AcpClientSettings::default(),
    profiles,
    |event| match event {
        AcpEvent::AgentThought { text } => println!("[思考] {text}"),
        AcpEvent::AgentMessage { text } => println!("[消息] {text}"),
        AcpEvent::Finished { .. } => println!("--- 回答完毕 ---"),
        AcpEvent::Failed { code, message } => eprintln!("{code}: {message}"),
        _ => {}
    },
)?;

println!("最终答案：{answer}");
```

上例中的 `load_profiles_from_app_settings()` 是宿主应用自己的配置读取函数，
不是 `lumina-acp` 提供的 API。

#### 3. 隔离任务

翻译、元数据解析等短任务使用：

```rust
let result = AcpService::prompt_isolated_restricted(
    task_prompt,
    "codex".into(),
    profiles,
    model_selection,
    Some("library:resolver".into()), // 仅日志追踪，不会发送给模型
)?;
```

该路径会创建短生命周期 session，不复用聊天历史，不传入视频上下文，也不开放
Lumina MCP 工具。无有效文本返回时会得到 `AcpErrorCode::NoOutput`，而不是一段
可被误解析为模型结果的提示文本。

#### 4. 字幕工作台的 `WorkshopPool`

字幕翻译和校对不是每个批次都重新启动一个 Agent。宿主应用会为一次工作创建
一个作业级 `WorkshopPool`：默认 4 个槽位，每个槽位复用一个无工具、无聊天历史的
隔离 session；传输层失败最多进行一次相同请求重试，作业完成或失败后显式关闭 pool。
`submit` 是阻塞调用，应放在 `spawn_blocking` 等阻塞线程中执行。

```rust
use std::sync::Arc;

use lumina_acp::{AgentProfilesHint, PoolConfig, WorkshopPool};

let pool = Arc::new(WorkshopPool::new(
    PoolConfig::new("codex", profiles),
    "subtitle-job-123".into(),
));
let translated = pool.submit(prompt, Some("batch=1".into()), None)?;
pool.shutdown();
```

`WorkshopPool::submit` 和 `open_conversation` 通过 `Arc` 接收 pool，目的是让多个
并发任务安全地竞争和占用不同的槽位。实际宿主代码通常把 pool 放进作业状态中，
在作业完成或失败时调用 `shutdown`；即使遗漏，`Drop` 也会执行兜底清理。

---

## 4. 源码目录：几块各管什么

读代码时先记住一句话目标，再按「谁依赖谁」往下钻。稳定对外 API 以
[`lib.rs`](./lib.rs) 根导出为准（`AcpService`、`AcpError`、`WorkshopPool` 等）；
旧平铺路径（`service` / `protocol` / `profile` / …）只是兼容 re-export。

### 4.1 一张图：怎么组成「一次 ACP」

```text
宿主 (Tauri / adapter)
        │  profiles hint、cwd、SessionEnvironment、事件回调
        ▼
┌───────────────────────────────────────────────────────────┐
│  runtime::AcpService          ← 对外总入口（活会话）         │
│    ├─ agent/     选哪个 Agent、命令怎么起、cwd 在哪、状态文案  │
│    ├─ wire/      JSON-RPC 怎么编/解（纯函数，不碰进程）        │
│    ├─ domain/    事件/设置/上下文 DTO + 宿主注入 port         │
│    └─ process/io/host …      stdio、权限、fs/terminal 回调   │
└───────────────────────────────────────────────────────────┘
        │  spawn + initialize → authenticate → session/new|resume
        │  session/prompt ↔ session/update …
        ▼
  Agent 子进程 (codex-acp / Claude / custom)

另线：jobs/（WorkshopPool、隔离 prompt）→ 复用 runtime，但不走聊天历史 / MCP
```

**依赖方向（新代码必须遵守）：**

```text
domain   ← 无依赖（纯数据）
agent    → domain
wire     → domain
runtime  → agent + wire + domain
jobs     → runtime + agent + domain
error    ← 全 crate 共用错误形状
```

唯一例外：`runtime/service` 里少数方法薄转发到 `jobs/isolated`，仅为保留旧
`AcpService` API，不是新业务依赖方向。

### 4.2 顶层模块一览（只记职责，不记每个文件）

| 目录 | 一句话目的 | 不管什么 |
| :--- | :--- | :--- |
| **`domain/`** | 跨边界的**稳定数据与宿主 port**：事件、状态、设置、视频上下文、`SessionEnvironment`。 | 不 spawn、不读写 stdio、不拼 ACP JSON 请求体。 |
| **`agent/`** | **启动前准备**：选哪个 profile、本机有没有二进制、最终 `LaunchSpec`、会话 cwd、给 UI 的中文状态。 | 不维护 live session，不跑 prompt 循环。 |
| **`wire/`** | **协议编解码**：请求/通知 JSON、session 参数、流式 update 解析、权限选项、路径脱敏。 | 不知道「哪个 Agent」、不持有子进程。 |
| **`runtime/`** | **活着的 Client**：spawn、握手、prompt 循环、取消/关闭、stdio IO、Agent→Client 的 fs/terminal。 | 不实现「选 Codex 还是 Claude」的产品策略（那是 agent）。 |
| **`jobs/`** | **短任务 / 作业池**：无历史无工具的隔离 prompt、字幕工作台 `WorkshopPool`、回复收集与 rollout 清理。 | 不是主聊天 UI 路径；聊天走 `runtime::AcpService::prompt`。 |
| **`error.rs`** | 统一 `{ code, message, details? }`；`message` 给用户，细节进 `details`/日志。 | — |

把一次「用户点发送」串起来：

1. **`agent`**：从 profiles hint 解析激活档案 → `resolve_launch` → `resolve_session_cwd`
2. **`runtime`**：按 `LaunchSpec` spawn → 用 **`wire`** 做 initialize / auth / session/new
3. **`wire` + `runtime`**：发 `session/prompt`，收 `session/update`，映射成 **`domain::AcpEvent`**
4. 需要本机文件/终端时：**`runtime/host`** 应答 Agent 的反向 RPC
5. 字幕翻译等批处理：**`jobs::WorkshopPool`** 开隔离槽位，仍底层走同一套 runtime/wire

### 4.3 各目录内部地图（粗览即可）

#### `domain/` — 数据与宿主约定

| 文件（约） | 作用 |
| :--- | :--- |
| `model.rs` | `AcpEvent`、`AcpStatus`、profile 相关 DTO、权限选项等 |
| `settings.rs` | `AcpClientSettings`、`PermissionMode`、`ThinkingLevel` |
| `context.rs` | `VideoPromptContext`（媒体锚点等，纯 DTO） |
| `environment.rs` | `SessionEnvironment`：宿主注入 MCP / snapshot 路径等 |

#### `agent/` — 「用谁、怎么起、在哪跑」

| 文件（约） | 作用 |
| :--- | :--- |
| `profile.rs` | Codex / Claude / Custom 档案；merge 前端 hint；解析当前激活项 |
| `discover.rs` | 在 PATH / `native/acp` / 开发树里找 bunx、codex-acp、codex |
| `launch.rs` | profile → `LaunchSpec`（program/args/env）；builtin Codex 启动优先级；选 auth 策略 |
| `workspace.rs` | 会话 `cwd`：本地目录优先，拒绝 http(s)，否则 AppData 工作区 |
| `status.rs` | 探测结果 → 中文 `AcpStatus.message` / 安装 hint |

#### `wire/` — 「线上长什么样」

| 文件（约） | 作用 |
| :--- | :--- |
| `codec.rs` | JSON-RPC envelope、入站分类 |
| `session.rs` | initialize / authenticate / new / resume / prompt / close 的 params 与解析 |
| `updates.rs` | 从 session update 抽出 thought / message / tool / plan |
| `permission.rs` | 权限请求选项与自动/选中策略辅助 |
| `sanitize.rs` | 路径与底层错误脱敏，避免泄漏进 UI `message` |

#### `runtime/` — 「会话活着时」

| 文件（约） | 作用 |
| :--- | :--- |
| `service.rs` | 对外门面：`connect` / `prompt` / `new_chat` / `cancel` / `close` / `status`… |
| `lifecycle.rs` | `LiveSession`：spawn、new/resume、旋转会话 |
| `prompt.rs` | 单次 prompt：超时、取消、流式收集 |
| `io.rs` | stdio 读写、按 request id 等待响应 |
| `inbound.rs` | 分发 Agent 推送的 update / permission |
| `host/` | 实现 Agent 回调的 `fs/*`、`terminal/*` |
| `process.rs` | 子进程 spawn/kill（无多余控制台窗口） |

#### `jobs/` — 「聊天以外的批处理」

| 文件（约） | 作用 |
| :--- | :--- |
| `pool.rs` | `WorkshopPool` / `PoolConfig`：作业级隔离 session 槽位 |
| `isolated.rs` | `prompt_isolated_restricted`、隔离模型发现 |
| `collector.rs` | 把流式块拼成 thinking + 正文 |
| `rollout.rs` | 隔离任务相关 Codex rollout 清理 |

更细的握手顺序、超时表、每个 `AcpService` 方法说明见 [`acp.md`](./acp.md)。

---

## 5. 核心协作与数据流向

### 普通聊天

```mermaid
sequenceDiagram
    autonumber
    participant UI as React / Tauri
    participant App as app adapter
    participant Svc as AcpService
    participant Agent as Agent Profile
    participant MCP as Lumina MCP

    UI->>App: 用户发送问题
    App->>App: 更新 agent-context.json
    App->>Svc: prompt(text, context, settings, profiles)
    alt 没有 live session
        Svc->>Agent: spawn + initialize
        Svc->>Agent: session/new 或 session/resume
        Agent->>MCP: 按 mcpServers 启动并 initialize
        MCP-->>Agent: instructions + tools/list
    end
    Svc->>Agent: session/prompt(resource_link + user text)
    opt Agent 需要 Lumina 上下文
        Agent->>MCP: tools/call
        MCP->>MCP: 读取 snapshot / 本地媒体数据
        MCP-->>Agent: 文本或图片结果
    end
    loop 流式响应
        Agent-->>Svc: session/update
        Svc-->>UI: AcpEvent::AgentThought / AgentMessage / ToolCall
    end
    Agent-->>Svc: session/prompt response
    Svc-->>UI: AcpEvent::Finished 或 Failed
```

关键点：`lumina-acp` 是 ACP Client，不直接执行 Lumina MCP 工具；MCP 工具由
Agent 根据 `tools/list` 自己调用。应用适配器负责写 snapshot，ACP 只负责把宿主
提供的 MCP 配置接入 `session/new|resume`。

### 隔离任务

```text
AgentInvoker
  → AcpAgentInvoker
  → job-scoped WorkshopPool
  → 复用隔离 Agent 进程和 session 槽位
  → NoTools MCP profile / 无聊天历史 / 无视频上下文
  → 返回一次性文本或 NoOutput
  → 业务层映射为翻译、元数据解析等固定错误
```

### 生命周期与错误

- `connect`：预热 Agent 和 session，不发送用户问题。
- `prompt`：复用 live session，发送一次 `session/prompt` 并收集流式事件。
- `new_chat`：关闭或旋转当前 session，开始新的聊天上下文。
- `request_cancel`：请求取消当前 prompt；超时后由 ACP 层终止子进程。
- `close_session`：结束 session 并清理 Agent 子进程。
- `AcpError` 使用 `{ code, message, details? }`；UI 只展示稳定的 `message`。

---

## 6. 连接原理与方法手册

[`acp.md`](./acp.md) 是本 crate 的深读文档，覆盖：

- **连接原理**：stdio JSON-RPC 传输、Codex 进程四阶启动顺序与环境补齐、
  `initialize → authenticate → session/new|resume` 握手、workspace 回退规则、
  超时表、僵尸进程防护；
- **方法含义**：`AcpService` 全部公开方法（`connect/prompt/new_chat/
  set_session_model/respond_permission/request_cancel/close_session/…`）、
  `WorkshopPool/PoolConfig`、`AcpEvent` 与 `AcpErrorCode` 速查；
- **最小案例**：聊天（连接→提问→关闭）、一次性隔离任务、取消与权限回填、
  宿主 `SessionEnvironment` 接线——复制即用。
