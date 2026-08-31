import { useEffect } from "react";
import { Sparkles, X } from "lucide-react";

import { PanelErrorBoundary } from "@/components/PanelErrorBoundary";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { useChatUiStore } from "../chatUiStore";
import { AcpPanel } from "./AcpPanel";

/** Layout dock, not a WebView overlay.
 * The libmpv HWND owns its rectangle, so chat must claim a sibling layout
 * column instead of visually floating over video pixels. It stays mounted
 * when hidden so an active ACP session survives close/reopen.
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
    <aside
      aria-hidden={!chatOpen}
      className={cn(
        "relative z-10 flex min-h-0 shrink-0 flex-col overflow-hidden bg-card",
        "transition-[width,border-color] duration-200 ease-out",
        chatOpen
          ? "w-[min(100vw,420px)] border-l border-border"
          : "w-0 pointer-events-none border-l-0",
      )}
    >
      <div className="flex min-h-0 w-[min(100vw,420px)] flex-1 flex-col">
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
      </div>
    </aside>
  );
}
