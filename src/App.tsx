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

import { PanelErrorBoundary } from "@/components/PanelErrorBoundary";
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

function SidebarTabPanel({ tab }: { tab: SidebarTab }) {
  switch (tab) {
    case "playlist":
      return <PlaylistPanel />;
    case "transcript":
      return <TranscriptPanel />;
    case "notes":
      return <NotesPanel />;
    case "chapters":
      return <ChaptersPanel />;
    case "acp":
      return <AcpPanel />;
    default:
      return null;
  }
}

export default function App() {
  usePlayerEvents();
  usePlayerHotkeys();
  useProgressPersistence();

  const fullscreen = useUiStore((s) => s.fullscreen);
  const sidebarTab = useUiStore((s) => s.sidebarTab);
  const setSidebarTab = useUiStore((s) => s.setSidebarTab);
  const activeTab = TABS.find((tab) => tab.id === sidebarTab);

  return (
    <TooltipProvider>
      <AppShell>
        <div className="flex min-h-0 flex-1 overflow-hidden">
          {/* Playback column: surface + chrome. HWND binds to VideoSurface only. */}
          <PanelErrorBoundary
            scope="playback"
            title="播放区域加载失败"
            hint="侧边栏与其它功能仍可使用。请重试；若仍失败请重启应用。"
            className="relative z-0 flex min-h-0 min-w-0 flex-1 flex-col"
          >
            <div className="relative z-0 flex min-h-0 min-w-0 flex-1 flex-col">
              <VideoSurface />
              <PlayerBar />
            </div>
          </PanelErrorBoundary>

          {!fullscreen ? (
            <aside className="relative z-10 flex w-[380px] shrink-0 flex-col border-l border-border bg-card">
              <PanelErrorBoundary
                scope="sidebar:media"
                title="媒体信息加载失败"
                className="shrink-0"
              >
                <MediaInfoPanel />
              </PanelErrorBoundary>
              <PanelErrorBoundary
                scope="sidebar:tracks"
                title="音轨/字幕加载失败"
                className="shrink-0"
              >
                <TrackMenus />
              </PanelErrorBoundary>
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
              <PanelErrorBoundary
                scope={`sidebar:${sidebarTab}`}
                resetKey={sidebarTab}
                title={`${activeTab?.label ?? "面板"}加载失败`}
                hint={
                  sidebarTab === "acp"
                    ? "请尝试重试或重启应用；若反复失败，可在开发者工具中清除站点存储后重试。"
                    : undefined
                }
                className="flex min-h-0 flex-1 flex-col overflow-hidden"
              >
                <SidebarTabPanel tab={sidebarTab} />
              </PanelErrorBoundary>
            </aside>
          ) : null}
        </div>
      </AppShell>
    </TooltipProvider>
  );
}
