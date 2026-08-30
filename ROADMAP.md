# ROADMAP

| Phase | Focus |
|-------|--------|
| 1 | Native Player（**完成**） |
| 2 | FFmpeg Media Inspection（**完成** — 项目本地 ffprobe + MediaInfo UI） |
| 3 | Subtitle / Transcript（**完成** — 文本字幕文稿 + 点击 seek） |
| 4 | ASR（**完成** — 按需 whisper-cli，不预加载） |
| 5 | Codex ACP |
| 6 | AI Video Reader |
| 7 | Multimodal Video Understanding |

## Phase 1 验收摘要

- Native 画面：子 HWND + `wid`，非 HTML `<video>`
- 控制：Open / Play / Pause / Stop / Seek / Volume / Rate
- 状态：Rust 权威，Channel → Zustand 镜像
- 检查：`cargo check` / `clippy -D warnings` / `pnpm lint`

## Phase 3 摘要

- `ffmpeg.exe` 与 ffprobe 同目录（gitignore）
- `SubtitleService`：列轨、抽出、解析 SRT/ASS/VTT
- `TranscriptPanel`：高亮 + 点击 seek；支持外挂字幕
- 位图字幕可上画面；文稿需文本轨或 ASR

## Phase 4 摘要

- ASR **非必须**；`asr_status` / `asr_transcribe` 仅按需
- 启动不加载模型；点击按钮才 spawn whisper-cli
- 未配置时播放/字幕仍可用
