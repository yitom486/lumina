# ROADMAP

| Phase | Focus |
|-------|--------|
| 1 | Native Player（**完成** — Windows libmpv wid 嵌入 + 基本控制） |
| 2 | FFmpeg Media Inspection |
| 3 | Subtitle / Transcript |
| 4 | ASR |
| 5 | Codex ACP |
| 6 | AI Video Reader |
| 7 | Multimodal Video Understanding |

## Phase 1 验收摘要

- Native 画面：子 HWND + `wid`，非 HTML `<video>`
- 控制：Open / Play / Pause / Stop / Seek / Volume / Rate
- 状态：Rust 权威，Channel → Zustand 镜像
- 检查：`cargo check` / `clippy -D warnings` / `pnpm lint`
