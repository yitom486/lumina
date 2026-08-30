import { AppShell } from "@/layouts/AppShell";
import { MediaInfoPanel } from "@/features/media";
import {
  PlayerBar,
  PlaylistPanel,
  ResumeDialog,
  usePlayerEvents,
  usePlayerHotkeys,
  useProgressPersistence,
  useUiStore,
  VideoSurface,
} from "@/features/player";
import { TranscriptPanel } from "@/features/transcript";
import { TooltipProvider } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

export default function App() {
  usePlayerEvents();
  usePlayerHotkeys();
  useProgressPersistence();

  const fullscreen = useUiStore((s) => s.fullscreen);
  const sidebarTab = useUiStore((s) => s.sidebarTab);
  const setSidebarTab = useUiStore((s) => s.setSidebarTab);

  return (
    <TooltipProvider>
      <AppShell>
        <div className="flex min-h-0 flex-1">
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <VideoSurface />
            <PlayerBar />
          </div>

          {!fullscreen ? (
            <aside className="flex w-[380px] shrink-0 flex-col border-l border-border bg-card">
              <MediaInfoPanel />
              <div className="flex shrink-0 gap-1 border-b border-border px-2 py-1.5">
                <button
                  type="button"
                  className={cn(
                    "rounded-md px-2.5 py-1 text-xs",
                    sidebarTab === "playlist"
                      ? "bg-accent text-accent-foreground"
                      : "text-muted-foreground hover:bg-muted",
                  )}
                  onClick={() => setSidebarTab("playlist")}
                >
                  播放列表
                </button>
                <button
                  type="button"
                  className={cn(
                    "rounded-md px-2.5 py-1 text-xs",
                    sidebarTab === "transcript"
                      ? "bg-accent text-accent-foreground"
                      : "text-muted-foreground hover:bg-muted",
                  )}
                  onClick={() => setSidebarTab("transcript")}
                >
                  文稿
                </button>
              </div>
              {sidebarTab === "playlist" ? (
                <PlaylistPanel />
              ) : (
                <TranscriptPanel />
              )}
            </aside>
          ) : null}
        </div>
        <ResumeDialog />
      </AppShell>
    </TooltipProvider>
  );
}
