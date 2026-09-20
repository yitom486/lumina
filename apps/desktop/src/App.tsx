/**
 * Layout stacking (must keep):
 *
 *   AppShell
 *   ├─ header (+ ChatToggleButton: ★ + chat, parallel chrome)
 *   └─ main
 *      ├─ row
 *      │   ├─ player column (min-w-0 flex-1; omitted from the normal DOM flow while Settings is active)
 *      │   │   ├─ VideoSurface  ← ONLY this rect maps to libmpv HWND
 *      │   │   └─ PlayerBar     ← HTML only (transport / seek / volume)
 *      │   │   (fullscreen: pt-12 top strip + hover chrome above HWND)
 *      │   ├─ SettingsWorkspaceFrame ← Settings replaces the player workspace when non-fullscreen
 *      │   ├─ aside (sidebar)   ← playlist / transcript / notes / chapters / library
 *      │   └─ ChatDock           ← layout sibling; never overlays the HWND
 *
 * Never put upward-opening menus or center dialogs on PlayerBar — HWND always
 * paints above WebView. Online resource settings live under Settings.
 */

import { useEffect, useState } from "react";

import { PanelErrorBoundary } from "@/components/PanelErrorBoundary";
import { AppShell } from "@/layouts/AppShell";
import { FullscreenTopChrome } from "@/layouts/FullscreenTopChrome";
import { WorkspacePanelFrame } from "@/layouts/WorkspacePanelFrame";
import { WorkspaceRail, type WorkspaceRailItem } from "@/layouts/WorkspaceRail";
import {
  SettingsWorkspaceFrame,
} from "@/layouts/SettingsWorkspaceFrame";
import { ChatDock } from "@/features/acp/components/ChatDock";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import { ChaptersPanel } from "@/features/chapters";
import { MediaInfoPanel } from "@/features/media";
import { MediaLibraryPanel, useLibraryPlaybackRootSync } from "@/features/library";
import {
  SettingsContent,
  type SettingsCategoryId,
} from "@/features/settings";
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
import { TooltipProvider } from "@lumina/ui/tooltip";
import { cn } from "@lumina/ui/utils";
import {
  FileText,
  Layers3,
  Library,
  ListVideo,
  Settings,
  StickyNote,
} from "lucide-react";

export const WORKSPACE_RAIL_ITEMS: readonly WorkspaceRailItem[] = [
  { id: "playlist", label: "选集列表", icon: ListVideo },
  { id: "transcript", label: "文稿", icon: FileText },
  { id: "notes", label: "笔记", icon: StickyNote },
  { id: "chapters", label: "章节", icon: Layers3 },
  { id: "library", label: "媒体库", icon: Library },
  { id: "settings", label: "设置", icon: Settings },
];

export function isSettingsWorkspace(
  fullscreen: boolean,
  sidebarTab: SidebarTab,
): boolean {
  return !fullscreen && sidebarTab === "settings";
}

export function shouldRenderPlayerWorkspace(
  fullscreen: boolean,
  sidebarTab: SidebarTab,
): boolean {
  return !isSettingsWorkspace(fullscreen, sidebarTab);
}

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
      return null;
    case "settings":
      return null;
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
  const closeChat = useChatUiStore((s) => s.closeChat);
  const [settingsCategory, setSettingsCategory] =
    useState<SettingsCategoryId>("playback");
  const settingsWorkspace = isSettingsWorkspace(fullscreen, sidebarTab);
  const playerWorkspace = shouldRenderPlayerWorkspace(fullscreen, sidebarTab);
  const activeTab = WORKSPACE_RAIL_ITEMS.find((tab) => tab.id === sidebarTab);

  const ActiveTabIcon = activeTab?.icon;

  useEffect(() => {
    if (settingsWorkspace) closeChat();
  }, [closeChat, settingsWorkspace]);

  return (
    <TooltipProvider delayDuration={400}>
      {/* delayDuration 是转出配置：vendor TooltipProvider 故意不定默认值。 */}
      <AppShell>
        <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
          <div className="flex min-h-0 flex-1 overflow-hidden">
            {!fullscreen ? (
              <WorkspaceRail
                items={WORKSPACE_RAIL_ITEMS}
                activeTab={sidebarTab}
                onSelect={setSidebarTab}
              />
            ) : null}

            {playerWorkspace ? (
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
            ) : (
              <div className="hidden" aria-hidden="true">
                {/* The hook still reports a deterministic 0×0 HWND rectangle. */}
                <VideoSurface />
              </div>
            )}

            {settingsWorkspace ? (
              <SettingsWorkspaceFrame
                activeCategory={settingsCategory}
                onCategoryChange={setSettingsCategory}
                renderContent={(category) => (
                  <SettingsContent category={category} />
                )}
              />
            ) : null}

            {!fullscreen && !chatOpen && !settingsWorkspace ? (
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

            <div
              className={cn("contents", settingsWorkspace && "hidden")}
              aria-hidden={settingsWorkspace || undefined}
            >
              <ChatDock />
            </div>
          </div>
        </div>
      </AppShell>
    </TooltipProvider>
  );
}
