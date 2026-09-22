# 手搓 ACP wire 归档（教学代码，冻结）

> 状态：**冻结，不参编**。本目录在 `src/` 之外，cargo 不会编译这里的任何文件。
> 对应批次计划：`.plan/L2-acp-sdk-wire-migration.md`。

## 这是什么

`lumina-acp` 在接入官方 `agent-client-protocol` SDK 之前，手写 ACP 协议层的
完整实现：`initialize`/`session/*` 共 14 个方法的 JSON 构造与解析、
`session/update` 到 `AcpEvent` 的映射、权限请求解析与自动结果。

- `session.rs` — 会话生命周期 wire 助手（含 `codex-acp` 实测校准注释）。
- `updates.rs` — `session/update` 通知到应用事件的映射。
- `permission.rs` — `session/request_permission` 解析、自动批复与响应构造。

故意**没有**搬过来的两个文件（仍在 `src/wire/` 现役）：
- `codec.rs` — JSON-RPC 包络 framing（`jsonrpc/id/method/params`），无协议语义。
- `sanitize.rs` — 隐私脱敏（日志与错误 `details` 过滤），不是协议。

## 阅读顺序（对照现役实现）

1. 先读现役 `src/wire/session.rs` 的同名函数：签名刻意保持一致，
   内部改用官方 schema 类型构造/解析。
2. 再读本目录旧版：同样的函数名，手写 `serde_json::json!` 与指针漫游。
3. 对照点：`_meta` 透传、`resource_link`/`image` prompt 块、`data.details`
   错误透出、`ResumeOutcome` 分类——行为必须逐条一致，单测与
   `tests/chapter_session_blackbox.rs`（真 stdio 黑盒）是仲裁。

## 规则

- 冻结：不再修改（拼写错误也不修），保持“当时就是这样写的”。
- 不得被任何 `use`/`mod` 引用；若编译器在归档目录里找到你，说明放错了地方。
