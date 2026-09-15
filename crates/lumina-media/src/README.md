# lumina-media

`lumina-media` 是 Lumina 的本地多媒体探测与工具集成领域库。它通过系统级集成 `ffprobe` 与 `ffmpeg`，提供流媒体元数据分析、章节探测、视频帧抓取及相邻剧集发现等能力，**完全独立于 libmpv 播放引擎与桌面 UI**。

---

## 1. 模块定位与职责

在 Lumina 整体架构中，`lumina-media` 是所有媒体相关业务的基础支撑组件：

```text
[lumina-subtitle]   [lumina-asr]   [lumina-notes]   [lumina-library]
        \                |               /                 /
         \               |              /                 /
          ▼              ▼             ▼                 ▼
                         [ lumina-media ]
                                 │
                   (本地进程 spawn / CLI 封装)
                                 ▼
                     ffprobe / ffmpeg (native)
```

- **职责**：
  - 调用 `ffprobe` 深度探测容器格式、视频流、音频流、内嵌字幕流及章节（Chapters）。
  - 内置基于 `mtime + size` 的进程级 LRU 探查缓存，避免打开同一文件时多次重复运行 ffprobe 开销。
  - 提供单帧视频截取能力（基于 `ffmpeg`），为笔记锚点及上下文提供缩略图支持。
  - 计算音频波形标志（`audio_marks`）与静音区间。
  - 启发式查找同目录下的相邻多媒体文件（`siblings`），支持自动连续播放与剧集发现。
- **硬约束**：
  - 严禁依赖 `libmpv`，避免探查元信息与播放器状态发生死锁。
  - 进程级失败（如 ffprobe 异常退出）只在 `details` 记录调试信息，对外输出固定的高层业务错误。

---

## 2. 构建思路与设计原则

1. **分离播放与探测（Separate Probe from Playback）**：
   - 传统桌面播放器常依赖播放器核心（如 mpv 属性）拉取视频元信息，容易在视频未初始化完毕时引发状态不同步。`lumina-media` 使用本地绑定的 `ffprobe` 作为单独的数据探测源，使得媒体库扫描、字幕分析可以在后台独立、无头（Headless）运行。
2. **多层缓存优化（Probe Cache）**：
   - 打开媒体时，播放控制面板、字幕提取、文稿初始化都会读取媒体信息。通过全局 `ProbeCache`（键为规范化路径，结合文件修改时间和尺寸），将探查开销降至最低。
3. **静默进程控制（Cross-Platform Process Management）**：
   - 在 Windows 系统上 spawn 外部工具时，默认设置 `CREATE_NO_WINDOW` 标记，防止弹出黑框控制台；对 stderr 进行严格限长与脱敏，防止崩溃或信息泄露。

---

## 3. 对外使用指南

### 添加依赖

```toml
[dependencies]
lumina-media = { path = "../lumina-media" }
```

### 代码使用示例

#### 1. 探查本地视频文件与内嵌字幕流

```rust
use std::path::Path;
use lumina_media::{MediaInspector, StreamKind};

let inspector = MediaInspector::new();
let info = inspector.inspect(Path::new("D:/Movies/example.mkv"))?;

println!("视频时长: {:.2}s", info.duration);
println!("包含章节数量: {}", info.chapters.len());

for stream in info.streams.iter().filter(|s| s.kind == StreamKind::Subtitle) {
    println!("内嵌字幕: #{} 语言: {:?}, 编码: {}", stream.index, stream.language, stream.codec);
}
```

#### 2. 查找同目录下连续剧集

```rust
use std::path::Path;
use lumina_media::list_sibling_videos;

let current = Path::new("D:/Series/S01E02.mp4");
let siblings = list_sibling_videos(current);
// 返回同目录下排好序的其他视频列表（如 S01E01, S01E03...）
```

---

## 4. 内部子模块全景

`lumina-media/src/` 的主要实现模块如下；列表聚焦稳定的核心文件，内部辅助文件可随实现调整：

| 源码文件 | 模块名称 | 核心职责与导出项 |
| :--- | :--- | :--- |
| [`lib.rs`](./lib.rs) | 根模块 | 重新导出公开接口；定义领域抽象约束。 |
| [`service.rs`](./service.rs) | `service` | • `MediaInspector`: 主服务结构体。<br>• `ProbeCache`: 线程安全的 64 项 LRU 探测缓存管理器。 |
| [`ffprobe.rs`](./ffprobe.rs) | `ffprobe` | 调用本地 `ffprobe` 进程并解析其输出的 JSON；结构化转换为 `MediaInfo`。 |
| [`frame_capture.rs`](./frame_capture.rs) | `frame_capture` | 封装 `ffmpeg -ss ... -vframes 1` 命令，抓取指定时间戳的高质量单帧画面。 |
| [`audio_marks.rs`](./audio_marks.rs) | `audio_marks` | 使用 ffmpeg 提取音频波形能量、静音切分点（Silence Detect），供 AI 或时间轴分段使用。 |
| [`siblings.rs`](./siblings.rs) | `siblings` | `list_sibling_videos`: 基于扩展名过滤和自然排序算法发现同一目录下的相邻剧集。 |
| [`tools.rs`](./tools.rs) | `tools` | 探测本机环境及本地打包目录中的 `ffmpeg` 与 `ffprobe` 可执行文件位置与可用状态。 |
| [`process.rs`](./process.rs) | `process` | 跨平台子进程构建工具（Windows 隐藏窗口、超时保护、标准流捕获与截断）。 |
| [`model.rs`](./model.rs) | `model` | • `MediaInfo`: 媒体时长、码率、元标签。<br>• `MediaStream`: 视频、音频、字幕轨道信息。<br>• `MediaChapter`: 容器章节打点。<br>• `StreamKind`: Video/Audio/Subtitle/Unknown。 |
| [`error.rs`](./error.rs) | `error` | • `MediaError` 与 `MediaErrorCode`（`ToolNotFound`, `ProbeFailed`, `InvalidMedia` 等）。 |

---

## 5. 核心协作与数据流向

```mermaid
flowchart LR
    Caller[调用方: Subtitle / Library / Tauri] -->|调用 inspect(path)| Inspector[MediaInspector]
    Inspector -->|检查 mtime/size| Cache{ProbeCache 命中?}
    Cache -- 是 -->|直接返回| Result[MediaInfo]
    Cache -- 否 -->|调用底层 CLI| Probe[ffprobe 模块]
    Probe -->|创建后台静默进程| Proc[process::run_command]
    Proc -->|执行外部工具| Bin[ffprobe.exe]
    Bin -->|JSON 输出| Probe
    Probe -->|反序列化与解析| Inspector
    Inspector -->|写入缓存| Cache
    Inspector -->|返回| Result
```
