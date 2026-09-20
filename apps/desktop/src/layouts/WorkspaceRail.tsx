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
      className="relative z-10 flex w-[5.75rem] shrink-0 flex-col items-center border-r border-border bg-surface-elevated px-2 py-3 max-[700px]:w-14 max-[700px]:px-1.5"
    >
      <div className="flex w-full flex-col items-center gap-1.5">
        {items.map(({ id, label, icon: Icon }) => {
          const selected = activeTab === id;
          return (
            <Tooltip key={id}>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="default"
                  className={cn(
                    "group flex h-14 w-full flex-col gap-1 rounded-lg px-1 py-2 text-[10px] font-medium leading-tight transition-[background-color,color,box-shadow] max-[700px]:size-10 max-[700px]:gap-0 max-[700px]:rounded-md",
                    selected
                      ? "border-l-2 border-ai bg-ai-muted text-ai shadow-sm hover:bg-ai-muted"
                      : "border-l-2 border-transparent text-muted-foreground hover:bg-surface-subtle hover:text-foreground",
                  )}
                  aria-label={label}
                  aria-pressed={selected}
                  aria-current={selected ? "page" : undefined}
                  data-active={selected ? "true" : "false"}
                  onClick={() => {
                    closeChat();
                    onSelect(id);
                  }}
                >
                  <Icon className="size-5 shrink-0 transition-transform group-hover:scale-105 max-[700px]:size-4" />
                  <span className="max-[700px]:sr-only">{label}</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="right">{label}</TooltipContent>
            </Tooltip>
          );
        })}
      </div>

      <div className="mt-auto w-full border-t border-border/70 pt-3">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="default"
              className={cn(
                "group flex h-14 w-full flex-col gap-1 rounded-lg px-1 py-2 text-[10px] font-medium leading-tight max-[700px]:size-10 max-[700px]:gap-0 max-[700px]:rounded-md",
                chatOpen
                  ? "border-l-2 border-ai bg-ai-muted text-ai hover:bg-ai-muted"
                  : "border-l-2 border-transparent text-muted-foreground hover:bg-surface-subtle hover:text-foreground",
              )}
              aria-label={chatOpen ? "收起 AI 对话" : "打开 AI 对话"}
              aria-pressed={chatOpen}
              aria-current={chatOpen ? "page" : undefined}
              data-active={chatOpen ? "true" : "false"}
              onClick={toggleChat}
            >
              <Sparkles className="size-5 shrink-0 transition-transform group-hover:scale-105 max-[700px]:size-4" />
              <span className="max-[700px]:sr-only">AI 对话</span>
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
