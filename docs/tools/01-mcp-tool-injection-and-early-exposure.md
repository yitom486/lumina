# MCP 工具注入与提前暴露

本文说明 Lumina 的 MCP 工具是怎样进入 Agent 会话的，以及“提前暴露工具”到底意味着什么。

这里的“注入”不是把十个工具硬编码进系统提示词，而是把工具能力沿着 ACP/MCP 的标准链路交给 Agent：

~~~text
主应用写入播放快照
        ↓
ACP session/new 注入 mcpServers
        ↓
Agent 启动 lumina --lumina-mcp
        ↓
MCP initialize 返回稳定使用说明
        ↓
MCP tools/list 返回当前会话真正可调用的工具和 schema
        ↓
Agent 发起 tools/call
        ↓
Lumina policy 再次校验并执行
~~~

## 1. 三种容易混淆的“工具信息”

| 层 | 作用 | 能否单独完成工具注入 |
| --- | --- | --- |
| ACP session/new.mcpServers | 告诉 Agent 从哪里启动 MCP Server | 不能，它只提供连接入口 |
| MCP initialize.instructions | 告诉模型有哪些能力、什么时候使用、调用优先级是什么 | 不能，它不是工具 schema 注册表 |
| MCP tools/list | 注册工具名称、参数 schema、annotations 和当前可见性 | 是，Host 只有拿到这里的 schema 才能可靠地调用工具 |
| MCP tools/call | 执行一次具体调用 | 不是暴露阶段，而是执行阶段 |

因此，最可靠的设计是“双通道”：

1. 用 initialize.instructions 提前告诉模型工具目录和调用规则；
2. 用 tools/list 提前提供真正可调用的工具 schema；
3. 用 tools/call 时再做一次服务端 policy 校验。

只把工具名称写进提示词，模型可能“知道”工具存在，但 Host 没有调用 schema；只返回 schema，模型又可能不知道工具的优先级，先去调用网络搜索或错误地选择工具。

## 2. Lumina 怎样把 MCP Server 注入 ACP 会话

### 2.1 主应用先写快照

播放进度、当前媒体、字幕窗口、剧集元数据和能力开关先由主应用写入脱敏快照：

~~~text
.lumina/agent-context.json
~~~

MCP 子进程不直接持有 libmpv handle，也不跨进程读取播放器内部状态。它读取快照，因此工具调用不会把播放器 UI 锁住。

相关实现：

- crates/lumina-mcp/src/snapshot.rs
- crates/lumina-mcp/src/build.rs
- 主应用 ACP 锚点和媒体切换逻辑

### 2.2 生成 MCP Server 启动项

crates/lumina-mcp/src/lib.rs 中的 lumina_mcp_server_entry 会把当前可执行文件包装成一个 MCP Server 启动项：

~~~rust
pub fn lumina_mcp_server_entry(snapshot_path: &Path) -> Value {
    let executable = std::env::current_exe()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| "lumina".into());

    json!({
        "name": "lumina",
        "command": executable,
        "args": ["--lumina-mcp"],
        "env": [{
            "name": CONTEXT_FILE_ENV,
            "value": snapshot_path.to_string_lossy(),
        }]
    })
}
~~~

这个 JSON 只描述“怎样启动服务器”。它不是工具清单，也不会直接把工具定义放进系统提示词。

### 2.3 在 session/new 中注入 MCP Server

lumina-acp 创建新会话时，把 lumina_mcp_servers(snapshot_path) 生成的配置放入 ACP session/new 参数：

~~~rust
let mcp_servers = environment.mcp_servers(snapshot_path)?;

let params = session_new_params(
    cwd,
    mcp_servers,
    model,
    mode,
);

write_request(stdin, "session/new", params)?;
~~~

上面是调用链的简化表示，实际入口在：

- crates/lumina-acp/src/runtime/lifecycle.rs
- apps/desktop/src-tauri/src/acp/adapter.rs

默认 Agent（codex-acp）随后会启动：

~~~text
lumina --lumina-mcp
~~~

并通过 stdio 建立 MCP JSON-RPC 连接。

## 3. 提前暴露的第一部分：initialize.instructions

MCP 初始化成功后，Lumina 可以返回稳定的使用说明。它适合放：

- 工具目录的概览和一句话用途；
- 当前集剧情问题应该优先查哪些工具；
- 当前集与其他集问题的工具选择规则；
- 需要画面理解时优先调用截图工具；
- 不允许在本地媒体工具可用时先调用网络搜索；
- 工具存在但没有出现在本次 tools/list 中时，不得手写调用。

示意代码如下：

~~~rust
const STABLE_INSTRUCTIONS: &str = r#"
Lumina 工具使用规则：

1. 当前集剧情问题：优先调用 lumina_get_library_context 和
   lumina_get_transcript_window。
2. 其他集的剧情问题：调用 lumina_get_episode_transcript。
3. 需要确认画面内容：调用 lumina_capture_frames。
4. 不要在本地媒体工具可用时先调用网络搜索。
5. 工具目录中存在、但本次 tools/list 没有返回的工具，视为当前会话未开放，
   不得手写工具名或猜测参数。
"#;

fn initialize_result() -> Value {
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {
            "tools": { "listChanged": false }
        },
        "instructions": STABLE_INSTRUCTIONS,
    })
}
~~~

initialize.instructions 的定位是“稳定的行为指引”，不是权限系统。提示词说可以调用某个工具，并不代表该工具真的开放。

## 4. 提前暴露的第二部分：tools/list

Agent 连接成功后会调用 tools/list。Lumina 先根据 profile 和快照能力裁剪目录，再返回每个工具的完整 schema：

~~~text
tools/list
  → ToolPolicy::tools(snapshot)
  → 当前 profile 可见的工具名称
  → tool_json(name)
  → name / description / inputSchema / annotations
~~~

当前 profile 由 LUMINA_MCP_TOOL_PROFILE 控制：

- Chat：普通对话工具集，并根据字幕、视觉和批注能力继续裁剪；
- SubtitleWorkshop：字幕读取和写入工具；
- MetadataResolver：不开放 Lumina 工具；
- NoTools：明确的无工具会话。

实现位置：

- crates/lumina-mcp/src/policy.rs
- crates/lumina-mcp/src/server.rs
- crates/lumina-mcp/src/tools.rs

tools/list 的返回结果是 Host 注册工具的事实来源。例如一个只读工具应包含类似结构：

~~~json
{
  "name": "lumina_get_transcript_window",
  "description": "读取当前播放位置附近的字幕窗口",
  "inputSchema": {
    "type": "object",
    "properties": {
      "radiusSec": { "type": "number" }
    }
  },
  "annotations": {
    "readOnlyHint": true
  }
}
~~~

这里的 schema 不是给模型“参考一下”的文本，而是 Host 形成可调用工具定义的正式输入。

## 5. tools/call 仍然必须二次校验

不能因为工具已经出现在 tools/list，就信任客户端随后发来的任意工具名。Lumina 对每一次调用再次检查：

~~~rust
pub fn check(
    &self,
    snapshot: &LuminaMcpSnapshot,
    name: &str,
) -> Result<(), String> {
    if self.tools(snapshot).contains(&name) {
        return Ok(());
    }

    if is_known_tool(name) {
        return Err("该工具未对当前任务开放".to_string());
    }

    Err(format!("Unknown tool: {name}"))
}
~~~

这形成了一个重要的不变量：

~~~text
tools/list 看得见的工具 ⊇ tools/call 可以调用的工具
~~~

实际实现中两者都消费同一个 ToolPolicy，因此不会出现“列表里没有但手写名称仍能调用”的权限绕过。

## 6. 用提示词把“知道工具”变成“正确调用工具”

工具 schema 解决“能不能调用”，初始化指引解决“应该先调用什么”。例如当前集剧情问题可以写成：

~~~text
当用户询问正在播放这一集的剧情时：

1. 先调用 lumina_get_library_context，了解剧集和当前集背景；
2. 再调用 lumina_get_transcript_window，读取当前播放锚点附近的台词；
3. 如果需要确认视觉事件，再调用 lumina_capture_frames；
4. 不要先使用网络搜索替代本地媒体上下文。
~~~

这里的“先后顺序”必须区分两种情况：

- 有依赖：先取得剧集或播放上下文，再决定后续窗口；
- 无依赖：多个只读查询可以一次提交，是否并行执行由下一篇文档介绍的调度门决定。

## 7. 常见错误

### 只改提示词，不改 tools/list

模型会知道一个工具名，但 Host 没有正式 schema，最终可能无法调用，或者把参数猜错。

### 只改 tools/list，不改 initialize.instructions

模型能调用工具，却可能在剧情问题上先走通用知识或网络搜索，无法体现 Lumina 的媒体优先策略。

### 只改 tools/list，不改 ToolPolicy

工具可能被列出来，但后续调用会被拒绝；更严重的是，如果执行端不复用 policy，还可能出现未列出的工具被手写调用。

### 把 initialize.instructions 当作系统提示词权限

MCP instructions 是 Agent 连接阶段收到的指引，不应替代 Host 的工具注册和服务端鉴权。真正的开放范围仍由 tools/list 和 tools/call 双重控制。

## 8. 排障顺序

遇到“模型说没有工具”时，按下面顺序查：

1. 是否走了 ACP session/new，并且参数中包含 Lumina MCP Server；
2. Agent 是否真的启动了 lumina --lumina-mcp；
3. initialize 是否成功返回 instructions；
4. tools/list 是否返回目标工具；
5. 当前 LUMINA_MCP_TOOL_PROFILE 和快照 capabilities 是否过滤了该工具；
6. tools/call 是否被 ToolPolicy 拒绝；
7. 最后才判断模型是否没有遵守调用指引。

不要先假设工具没有注入。必须把“没有启动 MCP”“工具被 profile 过滤”“模型知道但没有调用”区分开。

## 9. 已实测的时序缺口：session/new 不等 MCP 列出

2026-09-16 双边日志对出的实测（Lumina 日志 + `lumina-mcp.log`）：

```text
13:13:50.246  session/new completed
13:13:50.283  MCP 子进程启动（+37ms）
13:13:50.288  tools_list_served chat tools=8（+42ms）
```

即 `session/new` 返回只代表 Agent thread 建好，**不代表 MCP 已经列出**。首轮 prompt 可以在工具目录到达前就发出去，模型只能拿原生工具（网络搜索）作答；几十毫秒到几秒后 MCP 才列出，后续轮次反而正常——看起来像“会话污染”，实际是迟到。

推论：

- 不要用“首轮没调工具”证明“注册坏了”，先对 `lumina-mcp.log` 里该会话的 `tools_list_served` 时间和首答时间；
- `session/new` 的耗时（常见 5 秒左右）主要花在 Agent 建 thread 和模型握手，不是等 MCP；
- 想从 Lumina 侧“等注入成功再建会话”做不到：`session/new` 响应里不带 MCP 状态，App Server 的 listing 事件观测不到，只能事后对日志。

## 10. 每轮 prompt 的工具触发头（与时序无关的通道）

既然 MCP 通道的到达时机没保证，`session/prompt` 里加了一段固定触发头（`crates/lumina-acp/src/wire/session.rs` 的 `TOOL_TRIGGER_HEADER`），永远放在第 0 块、用户原文之前：

- 点名 8 个 Chat 工具（一工具一短语，字幕工坊写工具不在内）；
- 剧情类问题禁止先网络搜索；
- 若 `tools/list` 暂无 lumina 工具，要求模型直接说明接入未完成，而不是编造或静默走 web。

它只有名字和触发规则，完整版永远只在 `initialize.instructions` 里，不搞两套权威。单测锁住位置（第 0 块）、措辞（8 个名字全在）和无泄漏（无 `mediaPath` 等内部字段）。

## 11. 观测手段（去哪里看）

- Lumina 日志：`registering Lumina MCP for ACP session`（确认配置送达：command/args/env/profile）、`session/new completed`（耗时不代表等了 MCP）。
- `lumina-mcp.log`（`LUMINA_MCP_DIAGNOSTIC_LOG` 指向，App 端在 `adapter.rs` 里配）：`server_started` / `initialize_received` / `tools_list_served <profile> tools=<N>` / `tool started|finished`。`no-tools` 会话列出 0 是符合设计的（隔离任务），不要把它当成聊天会话注册失败。
- ACP 侧 `tool_call observed` 日志：只证明模型“宣布”了调用；真正的执行起止看上一条。
- 仍在 App Server 肚子里、观测不到的：它何时 spawn 我们的进程、列出后是否缓存、下次是否重列。需要时用 `/mcp` 问它自己，或看任务管理器里 lumina MCP 子进程在不在。

