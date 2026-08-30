/**
 * Layout stacking (must keep):
 *
 *   AppShell
 *   └─ row
 *      ├─ column (min-w-0 flex-1)
 *      │   ├─ VideoSurface  ← ONLY this rect maps to libmpv HWND
 *      │   └─ PlayerBar     ← HTML only (transport / seek / volume)
 *      └─ aside (sidebar)   ← HTML only (media info / tracks / tabs)
 *
 * Never put upward-opening menus on PlayerBar — HWND always paints above WebView.
 */

import { AppShell } from "@/layouts/AppShell";
import { AcpPanel } from "@/features/acp";
import { ChaptersPanel } from "@/features/chapters";
import { MediaInfoPanel } from "@/features/media";
import { NotesPanel } from "@/features/notes";
import {
  PlayerBar,
  PlaylistPanel,
  TrackMenus,
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
  { id: "acp", label: "对话" },
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
        <div className="flex min-h-0 flex-1 overflow-hidden">
          {/* Playback column: surface + chrome. HWND binds to VideoSurface only. */}
          <div className="relative z-0 flex min-h-0 min-w-0 flex-1 flex-col">
            <VideoSurface />
            <PlayerBar />
          </div>

          {!fullscreen ? (
            <aside className="relative z-10 flex w-[380px] shrink-0 flex-col border-l border-border bg-card">
              <MediaInfoPanel />
              <TrackMenus />
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
              <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                {sidebarTab === "playlist" ? <PlaylistPanel /> : null}
                {sidebarTab === "transcript" ? <TranscriptPanel /> : null}
                {sidebarTab === "notes" ? <NotesPanel /> : null}
                {sidebarTab === "chapters" ? <ChaptersPanel /> : null}
                {sidebarTab === "acp" ? <AcpPanel /> : null}
              </div>
            </aside>
          ) : null}
        </div>
      </AppShell>
    </TooltipProvider>
  );
}
