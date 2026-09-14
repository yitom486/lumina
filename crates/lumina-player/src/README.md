# lumina-player

`lumina-player` 是 Lumina 的原生多媒体播放核心领域库。它通过 `libmpv`（基于 `libmpv2` 绑定）直接驱动操作系统的原生窗口句柄（Native Surface）进行硬件加速视频渲染，同时维护着高内聚的播放状态机与快照流。

---

## 1. 模块定位与职责

在 Lumina 架构中，`lumina-player` 承担真正的视音频播放引擎职责：

```text
[React 前端 / Webview]
        │ (Tauri Commands / Events)
        ▼
[apps/desktop] (提供 Native Window HWND/NSView 句柄)
        │
        ▼
 [lumina-player] ──(PlayerService 状态机)
        │
        ▼
 [mpv::LibMpvPlayer] ──(C-FFI / libmpv2)──► libmpv.dll / libmpv.dylib (Native 硬件加速渲染)
```

- **职责**：
  - 承载 Native Surface 挂载：通过接收系统级原生窗口标识符（`wid`），使 libmpv 直接将像素渲染到窗口背景，**坚决杜绝 HTML `<video>` 或 Canvas 逐帧拷贝伪装播放**。
  - 维护单一真实数据源状态机（`PlayerSnapshot`）：精确聚合播放状态（`Idle` / `Loading` / `Playing` / `Paused` / `Error`）、当前时间戳、总时长、音量、倍速、视频宽高比及音视频字幕轨道信息。
  - 消费 `lumina-core::MediaSource`，统一处理本地文件播放与在线流媒体解析目标。
  - 异步事件轮询与转换：将 libmpv 的底层属性变动（Property Observe）和核心事件转化为上层易于处理的 `PlayerEvent`。
- **硬约束**：
  - 严防 FFI 内存与句柄泄漏：确保应用退出时执行优雅的 `shutdown()`。
  - 前端与上层 UI 绝对不感知 mpv handle、指针或 FFI 结构体；仅通过纯净的 `PlayerSnapshot` 交互。

---

## 2. 构建思路与设计原则

1. **原生表面渲染（True Native Rendering）**：
   - 网页容器自带的 Chromium 播放能力在解码高码率 4K HEVC/AV1、多轨道内嵌 ASS 样式字幕、音频透传等方面限制诸多。通过将窗口客户区绑定到 libmpv，获得了媲美 IINA / mpv 原生应用的解码性能与色彩准确度。
2. **状态快照模式（State Snapshot Pattern）**：
   - 播放器的进度更新非常频繁（每秒多次）。如果直接将底层每次细微变动广播给 React，会导致前端渲染管线崩溃。`PlayerService` 维护一个高频更新的内部快照 `PlayerSnapshot`，仅在状态跃迁、关键节点或受控节流时向上分发，保证了 UI 响应的高吞吐与低开销。
3. **软超时与防假死机制（Demux Timeout Protection）**：
   - 在线流媒体（如 YouTube）通过外部 hook 加载时，网络抖动可能导致 mpv 的 demux 阶段永久挂起。本模块设置了 `REMOTE_DEMUX_TIMEOUT`（12秒保护期），超时自动触发降级并向用户暴露明确的业务可读错误。

---

## 3. 对外使用指南

### 添加依赖

```toml
[dependencies]
lumina-player = { path = "../lumina-player" }
```

### 代码使用示例

#### 1. 初始化服务并绑定窗口句柄

```rust
use lumina_player::PlayerService;
use lumina_core::MediaSource;

let mut player = PlayerService::new();

// 传入 Tauri 窗口的平台原生句柄 (Windows HWND / macOS NSView / X11 Window ID)
let window_wid: i64 = 0x12345678;
player.attach_backend_with_wid(window_wid)?;

// 打开本地视频
let source = MediaSource::local("D:/Videos/demo.mp4");
player.open(&source)?;

// 播放控制
player.play()?;
player.seek(15.5)?; // 跳转到 15.5 秒
player.set_speed(1.25)?; // 1.25 倍速
```

#### 2. 读取当前播放快照

```rust
let snapshot = player.snapshot();
println!("状态: {:?}", snapshot.status);
println!("进度: {:.1}s / {:.1}s", snapshot.position, snapshot.duration);
println!("音量: {}%", snapshot.volume);
```

---

## 4. 内部子模块全景

`lumina-player/src/` 包含以下 5 个核心源码模块：

| 源码文件 | 模块名称 | 核心职责与导出项 |
| :--- | :--- | :--- |
| [`lib.rs`](file:///d:/project/rust/tauri/lumina/crates/lumina-player/src/lib.rs) | 根模块 | 重新导出公开接口；声明依赖边界。 |
| [`service.rs`](file:///d:/project/rust/tauri/lumina/crates/lumina-player/src/service.rs) | `service` | • `PlayerService`: 核心服务聚合，提供所有播放控制命令与快照管理。 |
| [`mpv.rs`](file:///d:/project/rust/tauri/lumina/crates/lumina-player/src/mpv.rs) | `mpv` | • `LibMpvPlayer`: 底层 `libmpv2::Mpv` 实例的拥有者。<br>• 配置 mpv 参数（vo, hwdec, wid, ytdl-format, 网络优化参数等）。<br>• 属性观察与底层 FFI 事件轮询泵。 |
| [`model.rs`](file:///d:/project/rust/tauri/lumina/crates/lumina-player/src/model.rs) | `model` | • `PlayerSnapshot`: 聚合播放器全部状态的纯数据结构。<br>• `PlayerState`: `Idle` / `Loading` / `Playing` / `Paused` / `Error`。<br>• `PlayerEvent`: 向上层派发的状态变动事件。 |
| [`source.rs`](file:///d:/project/rust/tauri/lumina/crates/lumina-player/src/source.rs) | `source` | 重新导出 `lumina_core::MediaSource` 与 `MediaSourceKind`，保持 API 语义整洁。 |
| [`error.rs`](file:///d:/project/rust/tauri/lumina/crates/lumina-player/src/error.rs) | `error` | • `PlayerError` 与 `PlayerErrorCode`（`NotInitialized`, `LoadError`, `CommandFailed`, `PropertyFailed`, `UnsupportedMedia`）。 |

---

## 5. 核心协作与数据流向

```mermaid
flowchart TD
    subgraph 外部调用 (Tauri/Desktop)
        Cmd[播放控制: open / play / seek]
        Poll[轮询状态: snapshot()]
    end

    subgraph lumina-player 内部
        Svc[PlayerService]
        Snap[(PlayerSnapshot)]
        Backend[mpv::LibMpvPlayer]
    end

    subgraph Native 运行时
        MPV[libmpv.dll / libmpv2]
        WinSurface[操作系统原生窗口 Wid]
    end

    Cmd --> Svc
    Poll --> Svc
    Svc -->|更新与读取| Snap
    Svc -->|下发指令| Backend
    Backend -->|FFI 调用| MPV
    MPV -.->|直接画面渲染| WinSurface
    MPV -- 事件与属性变化 --> Backend
    Backend -->|更新状态| Svc
```
