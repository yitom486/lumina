import { RateSelect } from "./RateSelect";
import { SeekBar } from "./SeekBar";
import { TransportControls } from "./TransportControls";
import { VolumeControl } from "./VolumeControl";

/** Control strip below the native video surface (never overlays HWND). */
export function PlayerBar() {
  return (
    <div className="flex shrink-0 flex-col gap-2 border-t border-border bg-card/80 px-3 py-2.5">
      <div className="flex items-center gap-3">
        <TransportControls />
        <SeekBar />
        <VolumeControl />
        <RateSelect />
      </div>
      <p className="text-[11px] text-muted-foreground">
        空格 播放/暂停 · ←→ 5 秒 · ↑↓ 音量 · M 静音 · F 全屏 · Esc 退出全屏
      </p>
    </div>
  );
}
