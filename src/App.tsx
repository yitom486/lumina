import { AppShell } from "@/layouts/AppShell";
import { AcpPanel } from "@/features/acp";
import { ChaptersPanel } from "@/features/chapters";
import { MediaInfoPanel } from "@/features/media";
import { NotesPanel } from "@/features/notes";
import {
  PlayerBar,
  PlaylistPanel,
  usePlayerEvents,
  usePlayerHotkeys,
  useProgressPersistence,
  useUiStore,
  type SidebarTab,
  VideoSurface,
} from "@/features/player";
import { TranscriptPanel } from "@/features/transcript";
import { TooltipProvider } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

const TABS: { id: SidebarTab; label: string }[] = [
  { id: "playlist", label: "列表" },
  { id: "transcript", label: "文稿" },
  { id: "notes", label: "笔记" },
  { id: "chapters", label: "章节" },
  { id: "acp", label: "ACP" },
];

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
              <div className="flex shrink-0 flex-wrap gap-1 border-b border-border px-2 py-1.5">
                {TABS.map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    className={cn(
                      "rounded-md px-2.5 py-1 text-xs",
                      sidebarTab === tab.id
                        ? "bg-accent text-accent-foreground"
                        : "text-muted-foreground hover:bg-muted",
                    )}
                    onClick={() => setSidebarTab(tab.id)}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>
              {sidebarTab === "playlist" ? <PlaylistPanel /> : null}
              {sidebarTab === "transcript" ? <TranscriptPanel /> : null}
              {sidebarTab === "notes" ? <NotesPanel /> : null}
              {sidebarTab === "chapters" ? <ChaptersPanel /> : null}
              {sidebarTab === "acp" ? <AcpPanel /> : null}
            </aside>
          ) : null}
        </div>
      </AppShell>
    </TooltipProvider>
  );
}
