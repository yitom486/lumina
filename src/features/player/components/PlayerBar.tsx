import { FullscreenToggleButton } from "@/layouts/AppShell";

import { RateSelect } from "./RateSelect";
import { ResumeChip } from "./ResumeChip";
import { SeekBar } from "./SeekBar";
import { TransportControls } from "./TransportControls";
import { VolumeControl } from "./VolumeControl";
import { useUiStore } from "../uiStore";

/** Control strip below the native video surface (never overlays HWND). */
export function PlayerBar() {
  const fullscreen = useUiStore((s) => s.fullscreen);

  return (
    <div className="flex shrink-0 flex-col gap-2 border-t border-border bg-card/80 px-3 py-2.5">
      <div className="flex items-center gap-3">
        <TransportControls />
        <div className="flex min-w-0 flex-1 flex-col">
          <ResumeChip />
          <SeekBar />
        </div>
        <VolumeControl />
        <RateSelect />
        {/* In fullscreen the title bar is hidden; keep exit control outside HWND. */}
        {fullscreen ? <FullscreenToggleButton /> : null}
      </div>
      <p className="text-[11px] text-muted-foreground">
        空格 播放/暂停 · ←→ 5 秒 · ↑↓ 音量 · M 静音 · F 全屏 · Esc 退出全屏
      </p>
    </div>
  );
}
