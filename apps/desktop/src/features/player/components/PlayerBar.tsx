import { useState } from "react";

import { FullscreenToggleButton } from "@/layouts/AppShell";
import { cn } from "@/lib/utils";

import { usePlaybackChromeReveal } from "../hooks/usePlaybackChromeReveal";
import { useUiStore } from "../uiStore";
import { SeekBar } from "./SeekBar";
import { TrackControlButtons } from "./TrackControlButtons";
import { TransportControls } from "./TransportControls";
import { VolumeControl } from "./VolumeControl";

/**
 * HTML chrome under the video placeholder.
 * Fullscreen: auto-hide, cinema gradient, inline track pickers (never over HWND).
 */
export function PlayerBar() {
  const fullscreen = useUiStore((s) => s.fullscreen);
  const { visible, pin, unpin, scheduleHide } = usePlaybackChromeReveal(fullscreen);
  const [menuPinned, setMenuPinned] = useState(false);
  const [volumeHover, setVolumeHover] = useState(false);

  const handleMenuOpenChange = (open: boolean) => {
    setMenuPinned(open);
    if (open) pin();
    else unpin();
  };

  if (fullscreen) {
    return (
      <div
        className={cn(
          "relative z-20 shrink-0 overflow-hidden transition-all duration-300 ease-out",
          visible
            ? "max-h-48 opacity-100"
            : "max-h-0 opacity-0 pointer-events-none",
        )}
        onMouseEnter={pin}
        onMouseLeave={() => {
          if (!menuPinned && !volumeHover) unpin();
          else scheduleHide();
        }}
      >
        <div
          className={cn(
            "border-t border-white/10 bg-gradient-to-t from-black/95 via-black/75 to-black/20 px-3 pb-3 pt-6 text-foreground",
            (menuPinned || volumeHover) && "pt-8",
          )}
        >
          <div className="flex items-center gap-2">
            <TransportControls />
            <div className="min-w-0 flex-1">
              <SeekBar />
            </div>
            <TrackControlButtons
              variant="bar"
              cinema
              onMenuOpenChange={handleMenuOpenChange}
            />
            <VolumeControl
              variant="hover-vertical"
              onHoverChange={(hovering) => {
                setVolumeHover(hovering);
                if (hovering) pin();
                else if (!menuPinned) unpin();
              }}
            />
            <FullscreenToggleButton />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="relative z-20 shrink-0 border-t border-border bg-card px-3 py-2.5">
      <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-3">
        <TransportControls />
        <SeekBar />
        <VolumeControl />
        <p className="col-span-full text-[11px] text-muted-foreground">
          空格 播放/暂停 · 单击画面 播停 · 双击全屏 · ←→ 5 秒 · ↑↓ 音量 · M 静音 · F / Esc 全屏
        </p>
      </div>
    </div>
  );
}
