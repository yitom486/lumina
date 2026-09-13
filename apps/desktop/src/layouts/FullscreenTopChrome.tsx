import { ChatToggleButton } from "@/features/acp/components/ChatToggleButton";

import { FullscreenToggleButton } from "./AppShell";

/**
 * Fullscreen-only top strip — lives above VideoSurface (outside HWND bounds).
 * Hover the top area (especially top-right) to reveal AI + exit controls.
 */
export function FullscreenTopChrome() {
  return (
    <div
      className="group absolute inset-x-0 top-0 z-20 h-12"
      aria-label="全屏顶部控制区"
    >
      <div
        className="pointer-events-none absolute inset-0 bg-gradient-to-b from-black/55 via-black/20 to-transparent opacity-0 transition-opacity duration-300 group-hover:opacity-100"
        aria-hidden
      />
      <div className="absolute inset-x-0 top-0 flex h-12 items-start justify-end px-3 pt-2">
        <div className="flex items-center gap-0.5 rounded-md border border-white/15 bg-black/60 p-0.5 opacity-0 shadow-lg backdrop-blur-md transition-opacity duration-200 group-hover:opacity-100 focus-within:opacity-100">
          <ChatToggleButton className="text-foreground hover:bg-white/10" />
          <FullscreenToggleButton />
        </div>
      </div>
    </div>
  );
}
