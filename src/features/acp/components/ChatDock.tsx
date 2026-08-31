import { useEffect } from "react";
import { Sparkles, X } from "lucide-react";

import { PanelErrorBoundary } from "@/components/PanelErrorBoundary";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { useChatUiStore } from "../chatUiStore";
import { AcpPanel } from "./AcpPanel";

/**
 * Floating chat dock — sibling to playback/sidebar layout, not a sidebar tab.
 * Stays mounted when hidden; survives fullscreen and sidebar tab switches.
 */
export function ChatDock() {
  const chatMounted = useChatUiStore((s) => s.chatMounted);
  const chatOpen = useChatUiStore((s) => s.chatOpen);
  const closeChat = useChatUiStore((s) => s.closeChat);

  useEffect(() => {
    if (!chatOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeChat();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [chatOpen, closeChat]);

  if (!chatMounted) return null;

  return (
    <>
      {chatOpen ? (
        <button
          type="button"
          className="fixed inset-0 z-40 bg-black/20 backdrop-blur-[1px] md:bg-black/10"
          aria-label="关闭对话面板"
          onClick={closeChat}
        />
      ) : null}

      <aside
        aria-hidden={!chatOpen}
        className={cn(
          "fixed inset-y-0 right-0 z-50 flex w-[min(100vw,420px)] flex-col border-l border-border bg-card shadow-2xl",
          "transition-transform duration-200 ease-out",
          chatOpen ? "translate-x-0" : "pointer-events-none translate-x-full",
        )}
      >
        <div className="flex h-10 shrink-0 items-center justify-between border-b border-border px-3">
          <div className="flex min-w-0 items-center gap-1.5 text-xs font-medium">
            <Sparkles className="size-3.5 shrink-0 text-amber-400/90" />
            <span className="truncate">AI 对话</span>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="收起对话"
            onClick={closeChat}
          >
            <X className="size-4" />
          </Button>
        </div>

        <PanelErrorBoundary
          scope="chat-dock"
          panelLabel="对话"
          className="flex min-h-0 flex-1 flex-col overflow-hidden"
        >
          <AcpPanel />
        </PanelErrorBoundary>
      </aside>
    </>
  );
}
