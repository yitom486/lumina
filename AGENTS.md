# Lumina — Agent Guide

AI Video Reader 桌面应用。当前做 **Phase 3：Subtitle / Transcript**（Phase 1–2 已完成）。

完整计划见 `ROADMAP.md`。执行计划在 `.plan/`（本地，不入库）。实现前读本文件与 `.cursor/rules/`。

## Tech Stack

- Tauri 2 + Rust + React + TypeScript + Vite + pnpm
- Player: libmpv（`libmpv2`）
- Inspect: 项目本地 ffprobe
- Subtitles: 项目本地 ffmpeg 抽出 + SRT/ASS/VTT 解析
- Zustand（播放镜像）+ TanStack Query（inspect / transcript）

## Architecture

```
React → Command/Channel → PlayerService → libmpv
              ↘ media_inspect → MediaInspector → ffprobe
              ↘ subtitle_* → SubtitleService → ffmpeg + parse
```

## Phase 3

In：文本字幕文稿、点击 seek、当前句高亮、外挂字幕。  
Out：ASR、OCR、位图字幕识别、翻译、RAG、SQLite、ACP。
