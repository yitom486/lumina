import { FullscreenToggleButton } from "@/layouts/AppShell";

import { ResumeChip } from "./ResumeChip";
import { SeekBar } from "./SeekBar";
import { TransportControls } from "./TransportControls";
import { VolumeControl } from "./VolumeControl";
import { useUiStore } from "../uiStore";

/**
 * HTML chrome under the video placeholder.
 * Must stay outside the HWND rect — no upward dropdowns here (those go in the sidebar).
 */
export function PlayerBar() {
  const fullscreen = useUiStore((s) => s.fullscreen);

  return (
    <div className="relative z-20 flex shrink-0 flex-col gap-2 border-t border-border bg-card px-3 py-2.5">
      <div className="flex items-center gap-3">
        <TransportControls />
        <div className="flex min-w-0 flex-1 flex-col">
          <ResumeChip />
          <SeekBar />
        </div>
        <VolumeControl />
        {fullscreen ? <FullscreenToggleButton /> : null}
      </div>
      <p className="text-[11px] text-muted-foreground">
        空格 播放/暂停 · 单击画面 播停 · 双击全屏 · ←→ 5 秒 · ↑↓ 音量 · M 静音 · F / Esc 全屏
        {fullscreen ? " · 音轨/字幕请先退出全屏，在右侧选择" : ""}
      </p>
    </div>
  );
}
