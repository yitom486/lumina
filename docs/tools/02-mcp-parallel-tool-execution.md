# MCP 工具的真正并行执行

“MCP 支持并行”不是一个单点开关。一次工具调用能否真正重叠执行，要同时通过模型发射、Codex 调度、ACP 传输和 Lumina 服务端四层：

~~~text
模型是否一次提交多个调用
        ↓
Codex App Server 是否允许这些工具共享执行门
        ↓
ACP/codex-acp 是否只是转发事件而没有额外排队
        ↓
Lumina MCP 是否有足够 worker，并且内部读写锁允许重叠
~~~

只有最后能观察到多个工具的实际执行区间重叠，才叫真正并行。

## 1. 四层并行条件

| 层 | 控制点 | 失败时的表现 |
| --- | --- | --- |
| 模型发射层 | 模型一次产生多个调用，或 code mode 中使用 Promise.all | 调用本身就一个接一个产生 |
| Codex App Server | supports_parallel_tool_calls 或工具的 readOnlyHint | 多个调用进入后，在执行门排队 |
| ACP/codex-acp | stdio JSON-RPC 和事件转发 | 事件可能按顺序显示，但不应决定 MCP 执行锁 |
| Lumina MCP | worker 池、读写锁、重工具限流 | 服务端收到调用后仍然排队或互斥 |

## 2. Lumina 服务端本身支持并行

### 2.1 stdio 不是“一次只能执行一个请求”

MCP 使用 stdio 传输，但 JSON-RPC 是带 request id 的消息协议。客户端可以连续写入多条 tools/call，不必等待前一条响应。

Lumina 的 crates/lumina-mcp/src/executor.rs 使用有界 worker 池：

~~~rust
pub(crate) struct TaskExecutor {
    sender: Option<mpsc::Sender<Job>>,
    workers: Vec<JoinHandle<()>>,
}

pub(crate) fn new(worker_count: usize) -> Self {
    // 每个 worker 从共享队列取一个 Job
}

pub(crate) fn submit<F>(&self, job: F) -> Result<(), String>
where
    F: FnOnce() + Send + 'static,
{
    self.sender.send(Box::new(job))?;
    Ok(())
}
~~~

当前实现使用固定数量的 worker，而不是为每个请求无限创建线程。只要客户端确实同时提交多个请求，独立工具就可以被多个 worker 同时执行。

### 2.2 Lumina 内部仍然要保护共享状态

并行 worker 不等于所有工具都可以互相覆盖。Lumina 的工具执行层应保持以下语义：

- 读取快照的工具可以并行；
- 修改字幕或持久化笔记的工具要独占；
- 截帧、音频分析等重工具使用单独的有界 limiter；
- 所有工具仍然经过 ToolPolicy 的二次授权。

这意味着 Lumina 自己的并行模型是“读共享、写独占、重任务限流”，而不是无条件放开全部工具。

## 3. 真正卡住本次调用的 Codex 执行门

Codex App Server 在工具进入 handler 之前会判断工具是否支持并行。当前 Codex 0.153.4 的 MCP handler 逻辑等价于：

~~~rust
fn supports_parallel_tool_calls(&self) -> bool {
    self.tool_info.supports_parallel_tool_calls
        || self
            .tool_info
            .tool
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.read_only_hint)
            .unwrap_or(false)
}
~~~

Codex 的并行执行门随后使用共享锁或独占锁：

~~~rust
let guard = if supports_parallel {
    lock.read().await
} else {
    lock.write().await
};

router.dispatch_tool_call(...).await;
~~~

因此：

- supports_parallel_tool_calls == true：多个工具可以同时进入 handler；
- readOnlyHint == true：即使服务器级 flag 没有设置，单个只读工具也可以共享执行门；
- 两者都没有：调用使用独占锁，后续调用排队。

官方实现还包含一个针对 read_only_hint 的单测，验证只读 annotations 可以在没有 server opt-in 的情况下允许并行：

<https://github.com/openai/codex/blob/rust-v0.153.4/codex-rs/core/src/tools/handlers/mcp.rs>

## 4. 为什么当前应优先使用 readOnlyHint

Lumina 最初可以在 MCP Server 配置中表达：

~~~json
{
  "supports_parallel_tool_calls": true
}
~~~

但当前 codex-acp 的 MCP 配置转换主要复制 command、args 和 env。它不会把这个 server-level 字段可靠地传入 Codex App Server：

<https://github.com/agentclientprotocol/codex-acp/blob/v1.7.0/src/CodexAcpClient.ts>

所以只在 lumina_mcp_server_entry 中添加 server flag 并不足够。

更稳妥的路径是让 Lumina 在 tools/list 的每个工具定义中返回 MCP annotations：

~~~rust
fn tool_annotations(name: &str) -> Value {
    if is_read_only_tool(name) {
        json!({
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
        })
    } else {
        json!({
            "readOnlyHint": false,
            "destructiveHint": true,
        })
    }
}
~~~

然后在 tool_json 中加入：

~~~rust
json!({
    "name": name,
    "description": description,
    "inputSchema": input_schema,
    "annotations": tool_annotations(name),
})
~~~

这里的分类必须按真实副作用审计：

- 播放上下文、剧集元数据、字幕读取、音频标记等读取工具：可以标记 readOnlyHint: true；
- 截帧和批注提议：只有在临时产物不会产生共享持久写入、路径不会冲突时，才标记为只读；
- 写字幕轨、保存批注等写工具：保持 readOnlyHint: false 或省略该字段。

readOnlyHint 是 MCP 的提示字段，但在当前 Codex 版本中会直接参与并行调度，所以对 Lumina/Codex 组合具有实际执行效果。

## 5. 模型并行与执行并行是两回事

即使 Codex 执行门已经打开，模型也必须同时提交多个调用。

### 5.1 原生顶层调用

如果模型和请求路径支持原生 parallel tool calls，模型可以一次产生多个工具调用。此时 Codex 可以为它们分别启动执行任务。

### 5.2 Code mode 中的 Promise.all

某些模型路径可能关闭原生顶层 parallel_tool_calls，但允许模型生成代码，在代码中显式并行调用：

~~~javascript
const results = await Promise.all([
  lumina_get_transcript_window({ radiusSec: 5, offsetSec: 5 }),
  lumina_get_transcript_window({ radiusSec: 5, offsetSec: 10 }),
  lumina_get_transcript_window({ radiusSec: 5, offsetSec: 15 }),
]);
~~~

这解释了为什么“模型说使用了 Promise.all”与“日志中工具一个接一个开始”可以同时出现：

~~~text
模型层：可能已经同时提交
Codex 层：由于 readOnlyHint 缺失，进入独占执行门后排队
~~~

因此不能只看模型回复中的文字，也不能只看 ACP UI 的卡片顺序，必须看实际执行开始和结束时间。

## 6. ACP 的单读循环不等于工具只能串行

lumina-acp 通常有一个 stdout reader 和一个 prompt 读取循环，这是为了保证 ACP 消息解析和 UI 事件顺序稳定：

~~~text
Agent stdout
    ↓
一个 read_one 循环
    ↓
session/update 事件转发
~~~

这个循环可能让 UI 按顺序显示事件，但它不应该决定 Codex App Server 内部多个 MCP handler 是否能够同时执行。执行是否重叠，应以 Codex 的并行执行门和 Lumina MCP 的 worker 时间区间为准。

同样，Lumina 的“一个 session 一次只处理一个 prompt”只限制会话轮次，不等于一轮内部的多个工具调用必须串行。

## 7. 日志应该怎样证明并行已经生效

建议同时记录三类时间点：

~~~text
收到 JSON-RPC tools/call
进入实际工具 handler
工具 handler 完成
~~~

三次只读调用真正并行时，应该看到类似：

~~~text
10:00:00.000 call A received / started
10:00:00.001 call B received / started
10:00:00.002 call C received / started
10:00:02.100 call A completed
10:00:02.200 call B completed
10:00:02.300 call C completed
~~~

而当前串行形状是：

~~~text
10:00:00.000 call A started
10:00:02.100 call A completed
10:00:02.106 call B started
10:00:04.200 call B completed
10:00:04.206 call C started
~~~

后一个形状中的几毫秒交接时间，通常是独占锁释放后下一个等待任务被唤醒，而不是模型在几毫秒内重新推理并生成新调用。

## 8. 验收清单

### 工具注册

- tools/list 中的只读工具包含 annotations.readOnlyHint: true；
- 写工具没有被错误地标记为只读；
- annotations 没有被 Agent adapter 转换过程丢失；
- 当前 Codex 版本确实读取 read_only_hint。

### 执行时序

- 三个独立只读调用在第一个完成前都出现实际 started；
- 总耗时接近最慢调用耗时，而不是三个耗时相加；
- Lumina worker 数量足以承接测试并发；
- 写工具仍然能够阻塞冲突的读写操作。

### 失败场景

- 工具未出现在 tools/list 时不能被手写调用；
- profile 或 capabilities 未开放时，tools/call 仍被拒绝；
- 取消会话时，所有并发中的调用都能收到取消信号并最终退出；
- Agent 连接断开时，worker 不会泄漏并继续写 stdout。

## 9. 结论

Lumina 的并行能力不是简单地把 MCP stdio 改成异步即可。完整方案是：

~~~text
tools/list 提供正确的 readOnlyHint
        ↓
Codex MCP handler 允许只读调用共享执行门
        ↓
Lumina TaskExecutor 使用多个 worker
        ↓
Lumina 读操作共享、写操作独占
        ↓
通过服务端 started/completed 日志验证实际重叠
~~~

对当前 codex-acp 路径而言，优先给真正只读工具增加 readOnlyHint: true，比只添加 server-level supports_parallel_tool_calls 更可靠。

