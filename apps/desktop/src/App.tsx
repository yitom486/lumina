/**
 * Layout stacking (must keep):
 *
 *   AppShell
 *   ├─ header (+ ChatToggleButton: ★ + chat, parallel chrome)
 *   └─ main
 *      ├─ row
 *      │   ├─ column (min-w-0 flex-1)
 *      │   │   ├─ VideoSurface  ← ONLY this rect maps to libmpv HWND
 *      │   │   └─ PlayerBar     ← HTML only (transport / seek / volume)
 *      │   │   (fullscreen: pt-12 top strip + hover chrome above HWND)
 *      │   └─ aside (sidebar)   ← playlist / transcript / notes / chapters / online
 *      │   └─ ChatDock           ← layout sibling; never overlays the HWND
 *
 * Never put upward-opening menus or center dialogs on PlayerBar — HWND always
 * paints above WebView. Online URL entry lives in the sidebar「在线」tab.
 */

import { PanelErrorBoundary } from "@/components/PanelErrorBoundary";
import { AppShell } from "@/layouts/AppShell";
import { FullscreenTopChrome } from "@/layouts/FullscreenTopChrome";
import { WorkspacePanelFrame } from "@/layouts/WorkspacePanelFrame";
import { WorkspaceRail, type WorkspaceRailItem } from "@/layouts/WorkspaceRail";
import { ChatDock } from "@/features/acp/components/ChatDock";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import { ChaptersPanel } from "@/features/chapters";
import { MediaInfoPanel } from "@/features/media";
import { MediaLibraryPanel, useLibraryPlaybackRootSync } from "@/features/library";
import { NotesPanel } from "@/features/notes";
import {
  PlayerBar,
  PlaylistPanel,
  TrackMenus,
  usePlayerEvents,
  usePlayerHotkeys,
  useProgressPersistence,
  useSessionPersistence,
  useSessionRestore,
  useUiStore,
  useWindowFullscreenSync,
  type SidebarTab,
  VideoSurface,
} from "@/features/player";
import { TranscriptPanel } from "@/features/transcript";
import { OnlineSourcePanel } from "@/features/ytdl/components/OnlineSourcePanel";
import { TooltipProvider } from "@lumina/ui/tooltip";
import { cn } from "@lumina/ui/utils";
import {
  FileText,
  Globe2,
  Layers3,
  Library,
  ListVideo,
  StickyNote,
} from "lucide-react";

const TABS: readonly WorkspaceRailItem[] = [
  { id: "playlist", label: "选集列表", icon: ListVideo },
  { id: "transcript", label: "文稿", icon: FileText },
  { id: "notes", label: "笔记", icon: StickyNote },
  { id: "chapters", label: "章节", icon: Layers3 },
  { id: "library", label: "媒体库", icon: Library },
  { id: "online", label: "在线资源", icon: Globe2 },
];

function SidebarTabPanel({ tab }: { tab: SidebarTab }) {
  switch (tab) {
    case "playlist":
      return <PlaylistPanel />;
    case "transcript":
      return (
        <>
          <PanelErrorBoundary
            scope="sidebar:transcript-media"
            panelLabel="媒体信息"
            className="relative z-20 shrink-0"
          >
            <MediaInfoPanel />
          </PanelErrorBoundary>
          <PanelErrorBoundary
            scope="sidebar:transcript-tracks"
            panelLabel="音轨/字幕"
            className="relative z-20 shrink-0"
          >
            <TrackMenus />
          </PanelErrorBoundary>
          <TranscriptPanel />
        </>
      );
    case "notes":
      return <NotesPanel />;
    case "chapters":
      return <ChaptersPanel />;
    case "library":
      return <MediaLibraryPanel />;
    case "online":
      return <OnlineSourcePanel />;
    default:
      return null;
  }
}

export default function App() {
  usePlayerEvents();
  usePlayerHotkeys();
  useProgressPersistence();
  useSessionPersistence();
  useSessionRestore();
  useWindowFullscreenSync();
  useLibraryPlaybackRootSync();

  const fullscreen = useUiStore((s) => s.fullscreen);
  const sidebarTab = useUiStore((s) => s.sidebarTab);
  const setSidebarTab = useUiStore((s) => s.setSidebarTab);
  const chatOpen = useChatUiStore((s) => s.chatOpen);
  const activeTab = TABS.find((tab) => tab.id === sidebarTab);

  const ActiveTabIcon = activeTab?.icon;

  return (
    <TooltipProvider delayDuration={400}>
      {/* delayDuration 是转出配置：vendor TooltipProvider 故意不定默认值。 */}
      <AppShell>
        <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
          <div className="flex min-h-0 flex-1 overflow-hidden">
            {!fullscreen ? (
              <WorkspaceRail
                items={TABS}
                activeTab={sidebarTab}
                onSelect={setSidebarTab}
              />
            ) : null}

            <PanelErrorBoundary
              scope="playback"
              panelLabel="播放区域"
              className="relative z-0 flex min-h-0 min-w-0 flex-1 flex-col"
            >
              <div
                className={cn(
                  "relative flex min-h-0 min-w-0 flex-1 flex-col",
                  fullscreen && "pt-12",
                )}
              >
                {fullscreen ? <FullscreenTopChrome /> : null}
                <VideoSurface />
                <PlayerBar />
              </div>
            </PanelErrorBoundary>

            {!fullscreen && !chatOpen ? (
              <WorkspacePanelFrame
                title={activeTab?.label ?? "工作区"}
                subtitle="与播放器并行的阅读面板"
                icon={ActiveTabIcon ? <ActiveTabIcon className="size-3.5" /> : null}
                className="w-[min(100vw,380px)]"
              >
                <PanelErrorBoundary
                  scope={`sidebar:${sidebarTab}`}
                  resetKey={sidebarTab}
                  panelLabel={activeTab?.label ?? "面板"}
                  className="flex min-h-0 flex-1 flex-col overflow-hidden"
                >
                  <SidebarTabPanel tab={sidebarTab} />
                </PanelErrorBoundary>
              </WorkspacePanelFrame>
            ) : null}

            <ChatDock />
          </div>
        </div>
      </AppShell>
    </TooltipProvider>
  );
}
