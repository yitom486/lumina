import type { ReactNode } from "react";
import { Maximize2, Minimize2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ChatToggleButton } from "@/features/acp/components/ChatToggleButton";
import { usePlayerStore, useUiStore } from "@/features/player";

type AppShellProps = {
  children: ReactNode;
};

export function FullscreenToggleButton() {
  const fullscreen = useUiStore((s) => s.fullscreen);
  const toggleFullscreen = useUiStore((s) => s.toggleFullscreen);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          onClick={() => void toggleFullscreen()}
          aria-label={fullscreen ? "退出全屏" : "全屏"}
        >
          {fullscreen ? (
            <Minimize2 className="size-4" />
          ) : (
            <Maximize2 className="size-4" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {fullscreen ? "退出全屏 (Esc / F)" : "全屏 (F)"}
      </TooltipContent>
    </Tooltip>
  );
}

/** Desktop app chrome: slim title bar + main content. */
export function AppShell({ children }: AppShellProps) {
  const fullscreen = useUiStore((s) => s.fullscreen);
  const fileLabel = usePlayerStore((s) => {
    if (!s.currentFile) return null;
    const parts = s.currentFile.split(/[/\\]/);
    return parts[parts.length - 1] || s.currentFile;
  });
  const status = usePlayerStore((s) => s.status);
  const playlist = usePlayerStore((s) => s.playlist);
  const playlistIndex = usePlayerStore((s) => s.playlistIndex);

  return (
    <div className="relative flex h-svh flex-col overflow-hidden bg-background text-foreground">
      {!fullscreen ? (
        <header className="flex h-11 shrink-0 items-center gap-3 border-b border-border bg-card px-4">
          <div className="flex min-w-0 items-baseline gap-2">
            <p className="text-sm font-semibold tracking-tight">Lumina</p>
            <p className="hidden text-xs text-muted-foreground sm:inline">
              AI Video Reader
            </p>
          </div>
          <div className="min-w-0 flex-1 truncate text-center text-xs text-muted-foreground">
            {fileLabel ?? "未打开文件"}
            {status && fileLabel ? (
              <span className="ml-2 opacity-70">· {status}</span>
            ) : null}
            {playlist.length > 1 && playlistIndex >= 0 ? (
              <span className="ml-2 opacity-70">
                · {playlistIndex + 1}/{playlist.length}
              </span>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-0.5">
            <ChatToggleButton />
            <FullscreenToggleButton />
          </div>
        </header>
      ) : null}
      <main className="flex min-h-0 flex-1 flex-col">{children}</main>
    </div>
  );
}
