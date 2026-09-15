# lumina-ai

`lumina-ai` 是 Lumina 的短周期、数据隔离式 AI 任务领域库。它专门处理无需长会话上下文的无状态批量 AI 任务（当前核心为**智能字幕翻译与润色**），通过依赖倒置端口 `AgentInvoker` 完成模型交互，确保不污染用户的正常聊天（Chat）历史记录。

---

## 1. 模块定位与职责

在 Lumina 架构中，`lumina-ai` 与 `lumina-acp` 形成清晰的职责区隔：

```text
[用户发起字幕翻译] ──────────────┐
                                 ▼
                         [ lumina-ai ]
                                 │
                         (使用 AgentInvoker 端口)
                                 ▼
                 [apps/desktop 适配注入的 ACP 调用]
                                 │ (作业级隔离 session pool)
                                 ▼
                          [ 大语言模型 ]
                                 │
                                 ▼
                        [ lumina-subtitle ]
                     (导出为新语言外挂字幕)
```

- **职责对比**：
  - `lumina-acp`：负责**长生命周期、有交互历史、带工具权限**的 Chat 侧边栏对话。
  - `lumina-ai`：负责**数据级隔离、批处理吞吐**的专项任务。每个字幕作业独立于 Chat 历史；宿主可以用作业级 session pool 复用隔离 Agent，作业结束后统一销毁，绝不向 Chat 注入几十页的字幕原文。
- **核心功能**：
  - 分块批处理（Chunked Batching）：将成百上千条字幕拆分为 40 条左右的批次（`TRANSLATE_BATCH_SIZE = 40`），避免单次 Prompt 击穿上下文或产生幻觉。
  - 严格结构化约束：利用 JSON Schema 约束模型按序输出带序号的翻译文本，确保时间戳轴精准对齐。
  - 自动落盘与回显：翻译完成后自动通过 `lumina-subtitle` 写入对应语言后缀的 `.srt` 外挂文件。

---

## 2. 构建思路与设计原则

1. **聊天会话零污染（Zero Chat History Pollution）**：
   - 如果直接在主聊天窗口让 Agent 翻译一部电影的 1500 句台词，会导致后续对话的 Token 消耗激增、上下文被噪音淹没。`lumina-ai` 强制走 `AgentInvoker::invoke_isolated`，完全剥离主会话上下文；底层可以通过 `WorkshopPool` 复用隔离 session，但不会复用 Chat session。
2. **纯依赖注入与面向端口编程（Port-Driven）**：
   - 本 crate 仅依赖 `lumina-core` 和 `lumina-subtitle`，完全不直接依赖 `lumina-acp`、子进程或网络库。具体的 AI 模型调用由宿主通过动态分发 `&dyn AgentInvoker` 传入。

---

## 3. 对外使用指南

### 在 Cargo workspace 中添加依赖

```toml
[dependencies]
lumina-ai.workspace = true
```

### 代码使用示例

```rust
use lumina_ai::{translate_and_export_track, AgentInvoker, IsolatedAgentTask, AgentTaskError};

// 1. 宿主提供实现了 AgentInvoker 的调用器
struct MyInvoker;
impl AgentInvoker for MyInvoker {
    fn invoke_isolated(&self, task: IsolatedAgentTask) -> Result<String, AgentTaskError> {
        // 调用底层的 ACP 适配层或直接请求 LLM
        Ok(r#"{"cues":[{"index":1,"text":"你好世界"}]}"#.into())
    }
}

let invoker = MyInvoker;

// 2. 发起字幕翻译流水线
let result = translate_and_export_track(
    "D:/Movies/interstellar.mkv",
    "embedded:0", // 源字幕轨 ID
    "zh-Hans",    // 目标语言
    None,         // 翻译上下文（简介＋人名表，可空）
    "default",    // Profile ID
    Some("gpt-4o"),
    None,
    &invoker,
    |update| {
        println!("当前进度: {}", update.message);
    },
    None, // 检查点工厂（断点续翻），不需要传 None
    "demo-job", // 作业 ID（日志关联用，不进模型）
);
```

---

## 4. 内部子模块全景

`lumina-ai/src/` 包含以下核心源码模块：

| 源码文件 | 模块名称 | 核心职责与导出项 |
| :--- | :--- | :--- |
| [`lib.rs`](./lib.rs) | 根模块 | 重新导出公开接口；定义隔离调用原则。 |
| [`translate.rs`](./translate.rs) | `translate` | • `translate_and_export_track`: 完整翻译端到端流水线（加载源字幕、批次切分、调用 AI、输出 `.srt`）。<br>• `translate_cues`: 纯内存字幕队列翻译（4 并发、序号对齐、人名 post-check＋单次重试）。<br>• `proofread_cues`: 同语言校对（不翻译不改轴）。<br>• 批处理 Prompt 构造与容错 JSON 反序列化解析。 |

---

## 5. 核心协作与数据流向

```mermaid
sequenceDiagram
    autonumber
    participant UI as 客户端 UI
    participant AI as lumina_ai::translate
    participant Port as &dyn AgentInvoker
    participant Sub as lumina_subtitle

    UI->>AI: translate_and_export_track(...)
    AI->>Sub: 加载源字幕 Transcript
    Sub-->>AI: 返回原始 Cues 列表 (如 120 句)

    loop 按 40 句一个批次分块
        AI->>Port: invoke_isolated(IsolatedAgentTask { prompt, model })
        Port-->>AI: 返回批量翻译 JSON
        AI->>AI: 校准索引，组装翻译后的 Cue 序列
        AI-->>UI: 进度回调 ("正在翻译第 1/3 批…")
    end

    AI->>Sub: export_sidecar_srt("movie.zh.srt", &translated)
    Sub-->>AI: 落盘成功
    AI-->>UI: 返回全新的 Transcript
```
