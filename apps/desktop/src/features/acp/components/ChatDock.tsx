import { useEffect } from "react";

import { PanelErrorBoundary } from "@/components/PanelErrorBoundary";
import { cn } from "@lumina/ui/utils";

import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import { AcpPanel } from "./AcpPanel";

/** Layout dock, not a WebView overlay.
 * The libmpv HWND owns its rectangle, so chat must claim a sibling layout
 * column instead of visually floating over video pixels. It stays mounted
 * when hidden so an active ACP session survives close/reopen.
 *
 * 无标题栏：标题栏只剩静态“AI 对话”四个字（X 已删），纯占高。
 * 收起走左侧导航栏的 AI 对话开关和 Esc；读屏用 aside 的 label。
 * 宽度由 App 行里的 RightPanelResizeHandle 统一调节（与章节等侧栏共用）。
 */
export function ChatDock() {
  const chatMounted = useChatUiStore((s) => s.chatMounted);
  const chatOpen = useChatUiStore((s) => s.chatOpen);
  const closeChat = useChatUiStore((s) => s.closeChat);
  const dockWidth = useChatUiStore((s) => s.dockWidth);

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
      aria-label="AI 对话"
      inert={!chatOpen}
      className={cn(
        "relative z-10 flex min-h-0 shrink-0 flex-col overflow-hidden border-l border-border bg-card",
        "transition-[width,border-color] duration-200 ease-out",
        chatOpen ? "max-w-[100vw]" : "w-0 pointer-events-none border-l-0",
      )}
      style={chatOpen ? { width: dockWidth } : undefined}
    >
      <PanelErrorBoundary
        scope="chat-dock"
        panelLabel="对话"
        className="flex min-h-0 w-full flex-1 flex-col overflow-hidden"
      >
        <AcpPanel />
      </PanelErrorBoundary>
    </aside>
  );
}
