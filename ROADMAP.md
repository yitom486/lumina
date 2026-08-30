# ROADMAP

| Phase | Focus |
|-------|--------|
| 1 | Native Player（**完成**） |
| 2 | FFmpeg Media Inspection（**完成** — 项目本地 ffprobe + MediaInfo UI） |
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

## Phase 2 摘要

- `src-tauri/native/ffmpeg/ffprobe.exe`（gitignore）
- `MediaInspector::inspect` → `media_inspect` Command
- 打开视频后 `MediaInfoPanel` 显示容器/流摘要
