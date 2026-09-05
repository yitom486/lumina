# ADR-0001 — Unified GPU Composition（Phase 8 去留）

- Status: **Accepted**（2026-09-06）
- Scope: 只做决策 + 原型计划；**不写任何 Phase 8 实现代码**（用户 P2-8 原令）。
- 前置证据：H-M4（平台矩阵）、H-M5（headless 基线，可比）。

## Context

播放器用 libmpv 子 HWND（`wid`）直出视频像素，WebView 的 HTML/CSS
（`z-index`、Portal、Tooltip、Dialog）跨不过 native window 边界。
当前靠视频安全区 + 收缩矩形 + 画面外控制栏绕行（`App.tsx` 注释即契约）。
Phase 8 想验证统一 GPU 合成：视频与 Web UI 同一合成树叠加，
同时保留硬解、零 CPU 回读、失败回退。

## 实证（本轮新查，非猜测）

1. `libmpv2 6.0.0`（Cargo.lock 锁死）的 render binding **只有 OpenGL**
  （vendored 源码 `src/mpv/render.rs`：`RenderParamApiType` 唯一变体 `OpenGl`；
   且该 crate `default = ["render"]`，我方 `libmpv2 = "6"` 已默认启用——
   原型无需加依赖即可试 OpenGL 路）。
2. D3D11 零拷贝纹理共享**不在该 binding 内**，需 `libmpv2-sys` raw FFI
   或自研绑定（工作量陡增，且碰 FFI 最危险区）。
3. pin 住的 mpv（`native/mpv/VERSION`：shinchiro 20260830）render API 本体可用，
   瓶颈不在 mpv。
4. 真正的未知数在另一头：Tauri/Wry **没有暴露**可供外部 GPU 纹理挂载的
   WebView2 合成面；透明 WebView + 输入命中 + DPI + 全屏 + 切屏 + device lost
   全是未验证项，大概率要平台补丁。这是 No-Go 的最大嫌疑项。
5. OpenGL 路即使走通，大概率仍是“自建 GL 窗口/子 HWND”——**解不了 overlay 问题**，
   只能算练手，不能算 Go。

## Decision

1. **现阶段判：实现侧 No-Go，Spike 侧 Go。** 不开 8.1+ 的实现分支；
   `WindowedHwnd` 保持唯一生产后端 + 安全区布局契约不动。
2. 8.0 Spike 立项条件（任一满足即开）：出现真实 overlay 需求阻塞发布、
   或有人能全职投入 1–2 周。Spike 盒子见下，超时未出结论即判 No-Go，
   回到安全区方案，不追加投入。
3. 验收阈值沿用 `ROADMAP.md` Phase 8（无 CPU 回读、硬解保持、丢帧≈基线、
   开销 +10% 内、浮层稳定、可回退）。比较基准用 H-M5 基线脚本同机同媒体重跑。

## Spike 盒子（8.0，最小原型计划）

- P0：基线复测（H-M5 脚本 + 1080p60/4K60 真文件，HWND 后端，记 CPU/GPU/丢帧/首帧/seek）。
- P1：OpenGL render 原型（`libmpv2::render` 现成 binding，自建 GL 上下文渲染到纹理，
  只验证解码→纹理链路，不碰合成）。
- P2：D3D11 共享纹理可行性（raw-sys FFI spike，限读：能否零拷贝拿到可共享句柄）。
- P3：WebView2 合成挂载调研（Wry 是否暴露 DCLayer/纹理目标；透明/命中/DPI/全屏/device lost 逐项 Yes/No）。
- Go 门：P2 + P3 同时 Yes，且纸面设计满足全部验收阈值。任一 No 即整体 No-Go。

## Consequences

- 好：不烧 6–12 周赌博；安全区方案继续可用；后人开 spike 有现成靶子。
- 坏：Tooltip/Dialog 盖视频的限制继续存在；新组件仍须守安全区注释。
- 若判 Go：按 ROADMAP 8.1–8.5 推进，且 `CompositedGpu` 必须与 `WindowedHwnd`
  共存一个发布周期以上，异常自动回退。

## Links

- `ROADMAP.md` Phase 8（远期研究总纲）
- `ARCHITECTURE.md` M5 矩阵 + Playback baseline 节
- `.plan/L2/H-P1-5-perf-baseline.md`（比较方法）
