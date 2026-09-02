# Whisper（可选，按需 ASR，不入库）

本目录 **不是** 打开视频的必要条件。只有用户点击「生成字幕 (ASR)」时才会查找并启动。

## 推荐方式：应用内一键下载

在文稿面板选择 Tiny / Base / Small，点「一键下载」：

- 引擎与模型写入本机应用数据目录（Windows：`%APPDATA%\lumina\whisper\`）
- 启动时**不会**预加载；仅在下载或转写时访问网络/磁盘
- 当前一键下载优先支持 Windows

## 需要放置（手动亦可）

| 文件 | 说明 |
|------|------|
| `whisper-cli.exe` | whisper.cpp Windows 构建 |
| `ggml-*.bin` | 模型，建议 `ggml-base.bin` |

## 推荐布局（开发态）

```
src-tauri/native/whisper/
  whisper-cli.exe
  models/
    ggml-base.bin
  README.md
  VERSION
```

也可把 model 直接放在本目录根下。运行时还会扫描 `%APPDATA%\lumina\whisper\` 与可执行文件旁的 `whisper/`。

## 行为

- 启动应用 **不会** 加载模型
- `asr_status`：是否可用、本地模型列表、可下载目录（tiny/base/small）
- `asr_install`：按需下载引擎 zip + 所选模型（用户点击才执行）
- `asr_transcribe`：首次转写才 spawn；可选 `range` / `modelId`；成功后写外挂字幕

## 手动获取示例

1. whisper.cpp Releases：`whisper-bin-x64.zip`（如 tag `b4938`）
2. Hugging Face `ggerganov/whisper.cpp`：`ggml-base.bin`
3. 放到上述路径后即可按需使用（一般无需重启；刷新文稿面板状态即可）
