import { useEffect, useState } from "react";

import { FullscreenToggleButton } from "@/layouts/AppShell";
import { cn } from "@lumina/ui/utils";

import { usePlaybackChromeReveal } from "../hooks/usePlaybackChromeReveal";
import { ensureSurfaceBounds } from "../surfaceBridge";
import { useUiStore } from "../uiStore";
import { SeekBar } from "./SeekBar";
import { TrackControlButtons } from "./TrackControlButtons";
import { TransportControls } from "./TransportControls";
import { VolumeControl } from "./VolumeControl";

/** Demo (osc-leave-bar): nominal HTML bar reserve height, tunable (px). */
export const OSC_DEMO_BAR_HEIGHT_PX = 56;

/**
 * HTML chrome under the video placeholder.
 * Fullscreen: auto-hide, cinema gradient, inline track pickers (never over HWND).
 */
export function PlayerBar() {
  const fullscreen = useUiStore((s) => s.fullscreen);
  const { visible, pin, unpin, scheduleHide } = usePlaybackChromeReveal(fullscreen);
  const [menuPinned, setMenuPinned] = useState(false);
  const [volumeHover, setVolumeHover] = useState(false);

  // Demo (osc-leave-bar): fullscreen only — bar show narrows the video face,
  // bar hide expands it to full bleed, via the existing set_bounds chain
  // (VideoSurface rect measurement). Windowed: no-op, never touch bounds.
  useEffect(() => {
    if (!fullscreen) return;
    let cancelled = false;
    const refresh = () => {
      if (!cancelled) void ensureSurfaceBounds();
    };
    const raf = requestAnimationFrame(() => refresh());
    const timers = [80, 250, 450].map((ms) => window.setTimeout(refresh, ms));
    return () => {
      cancelled = true;
      cancelAnimationFrame(raf);
      for (const id of timers) window.clearTimeout(id);
    };
  }, [fullscreen, visible]);

  const handleMenuOpenChange = (open: boolean) => {
    setMenuPinned(open);
    if (open) pin();
    else unpin();
  };

  if (fullscreen) {
    return (
      <div
        className={cn(
          "relative z-20 shrink-0 transition-all duration-300 ease-out",
          visible
            ? "max-h-48 overflow-visible opacity-100"
            : "max-h-0 overflow-hidden opacity-0 pointer-events-none",
        )}
        onMouseEnter={pin}
        onMouseLeave={() => {
          if (!menuPinned && !volumeHover) unpin();
          else scheduleHide();
        }}
      >
        <div
          className={cn(
            "border-t border-player-control/50 bg-gradient-to-t from-player-surface/95 via-player-surface/75 to-player-surface/20 px-3 pb-3 pt-6 text-player-control-foreground",
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
    <div className="relative z-20 shrink-0 border-t border-border/70 bg-player-chrome px-4 pb-3 pt-2.5">
      <div className="flex flex-col gap-2">
        <SeekBar />
        <div className="flex items-center justify-between gap-3">
          <TransportControls />
          <VolumeControl className="w-32" />
        </div>
        <p className="text-[11px] text-muted-foreground">
          空格 播放/暂停 · 单击画面 播停 · 双击全屏 · ←→ 5 秒 · ↑↓ 音量 · M 静音 · F / Esc 全屏
        </p>
      </div>
    </div>
  );
}
