import { Sparkles, type LucideIcon } from "lucide-react";

import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import { Button } from "@lumina/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@lumina/ui/tooltip";
import { cn } from "@lumina/ui/utils";

import type { SidebarTab } from "@/features/player";

export type WorkspaceRailItem = {
  id: SidebarTab;
  label: string;
  icon: LucideIcon;
};

type WorkspaceRailProps = {
  items: readonly WorkspaceRailItem[];
  activeTab: SidebarTab;
  onSelect: (tab: SidebarTab) => void;
};

/**
 * Narrow navigation rail for the sibling workspaces around the native player.
 * It only changes which existing panel is rendered; it never enters the HWND
 * rectangle or creates a transparent overlay above it.
 */
export function WorkspaceRail({
  items,
  activeTab,
  onSelect,
}: WorkspaceRailProps) {
  const chatOpen = useChatUiStore((state) => state.chatOpen);
  const closeChat = useChatUiStore((state) => state.closeChat);
  const toggleChat = useChatUiStore((state) => state.toggleChat);

  return (
    <nav
      aria-label="工作区导航"
      className="relative z-10 flex w-12 shrink-0 flex-col items-center border-r border-border bg-card/80 px-1.5 py-2"
    >
      <div className="flex flex-col items-center gap-1">
        {items.map(({ id, label, icon: Icon }) => {
          const selected = activeTab === id;
          return (
            <Tooltip key={id}>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className={cn(
                    "size-9 rounded-md",
                    selected
                      ? "bg-accent text-accent-foreground hover:bg-accent"
                      : "text-muted-foreground hover:bg-muted hover:text-foreground",
                  )}
                  aria-label={label}
                  aria-pressed={selected}
                  onClick={() => {
                    closeChat();
                    onSelect(id);
                  }}
                >
                  <Icon className="size-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="right">{label}</TooltipContent>
            </Tooltip>
          );
        })}
      </div>

      <div className="mt-auto border-t border-border/70 pt-2">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className={cn(
                "size-9 rounded-md",
                chatOpen
                  ? "bg-accent text-accent-foreground hover:bg-accent"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
              aria-label={chatOpen ? "收起 AI 对话" : "打开 AI 对话"}
              aria-pressed={chatOpen}
              onClick={toggleChat}
            >
              <Sparkles className="size-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="right">
            {chatOpen ? "收起对话 (Esc)" : "AI 对话"}
          </TooltipContent>
        </Tooltip>
      </div>
    </nav>
  );
}
