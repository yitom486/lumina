# Whisper（可选，按需 ASR，不入库）

本目录 **不是** 打开视频的必要条件。只有用户点击「生成文稿 (ASR)」时才会查找并启动。

## 需要放置

| 文件 | 说明 |
|------|------|
| `whisper-cli.exe` | [ggerganov/whisper.cpp](https://github.com/ggerganov/whisper.cpp) Windows 构建 |
| `ggml-*.bin` | 模型，建议先用小模型如 `ggml-base.bin` / `ggml-small.bin` |

## 推荐布局

```
src-tauri/native/whisper/
  whisper-cli.exe
  models/
    ggml-base.bin
  README.md
  VERSION
```

也可把 model 直接放在本目录根下。

## 行为

- 启动应用 **不会** 加载模型
- `asr_status`：报告是否可用（有 cli + 至少一个 model），并列出本地 `ggml-*.bin`
- `asr_transcribe`：首次调用才 spawn 进程；可选 `range` / `modelId`；用项目本地 ffmpeg 抽 16 kHz mono wav（可选时间窗 `-ss`/`-t`）；成功后写出媒体同目录外挂字幕（整片 `{stem}.asr.srt`，章节 `{stem}.asr_ch{id}.srt`）

> 应用内不提供一键下载模型；请自行放到 `native/whisper/`（体积与许可见上游文档）。

## 获取示例（自行下载）

1. 从 whisper.cpp Releases 取 Windows 二进制  
2. 从 Hugging Face `ggerganov/whisper.cpp` 取 `ggml-base.bin`  
3. 放到上述路径后重启应用即可按需使用
