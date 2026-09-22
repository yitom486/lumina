import { useEffect } from "react";
import { Sparkles } from "lucide-react";

import { PanelErrorBoundary } from "@/components/PanelErrorBoundary";
import { WorkspacePanelFrame } from "@/layouts/WorkspacePanelFrame";
import { cn } from "@lumina/ui/utils";

import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import { AcpPanel } from "./AcpPanel";

/** Layout dock, not a WebView overlay.
 * The libmpv HWND owns its rectangle, so chat must claim a sibling layout
 * column instead of visually floating over video pixels. It stays mounted
 * when hidden so an active ACP session survives close/reopen.
 *
 * 收起只有两个入口：左侧导航栏的 AI 对话开关和 Esc。标题栏不放 X，
 * 避免和导航栏重复。
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
      inert={!chatOpen}
      className={cn(
        "relative z-10 flex min-h-0 shrink-0 flex-col overflow-hidden",
        "transition-[width,border-color] duration-200 ease-out",
        chatOpen
          ? "w-[min(100vw,380px)] border-l border-border"
          : "w-0 pointer-events-none border-l-0",
      )}
    >
      <WorkspacePanelFrame
        title="AI 对话"
        icon={<Sparkles className="size-3.5" />}
        className="h-full w-[min(100vw,380px)] border-l-0"
      >
        <PanelErrorBoundary
          scope="chat-dock"
          panelLabel="对话"
          className="flex min-h-0 flex-1 flex-col overflow-hidden"
        >
          <AcpPanel />
        </PanelErrorBoundary>
      </WorkspacePanelFrame>
    </aside>
  );
}
