import { AppShell } from "@/layouts/AppShell";
import { MediaInfoPanel } from "@/features/media";
import {
  PlayerBar,
  ResumeDialog,
  usePlayerEvents,
  usePlayerHotkeys,
  useProgressPersistence,
  VideoSurface,
} from "@/features/player";
import { TranscriptPanel } from "@/features/transcript";
import { TooltipProvider } from "@/components/ui/tooltip";

export default function App() {
  usePlayerEvents();
  usePlayerHotkeys();
  useProgressPersistence();

  return (
    <TooltipProvider>
      <AppShell>
        <div className="flex min-h-0 flex-1">
          {/* Left: native video HWND region + controls below (never overlay HWND) */}
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <VideoSurface />
            <PlayerBar />
          </div>

          {/* Right: reader sidebar — outside HWND bounds */}
          <aside className="flex w-[380px] shrink-0 flex-col border-l border-border bg-card">
            <MediaInfoPanel />
            <TranscriptPanel />
          </aside>
        </div>
        <ResumeDialog />
      </AppShell>
    </TooltipProvider>
  );
}
