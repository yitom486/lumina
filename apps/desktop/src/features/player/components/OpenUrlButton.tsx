/** Control-bar shortcut: jump to sidebar「在线」tab (never open dialog over HWND). */

import { Link2 } from "lucide-react";

import { Button } from "@lumina/ui/button";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";

import { useUiStore } from "../uiStore";

export function OpenUrlButton() {
  const setSidebarTab = useUiStore((s) => s.setSidebarTab);
  const fullscreen = useUiStore((s) => s.fullscreen);
  const setFullscreen = useUiStore((s) => s.setFullscreen);
  const closeChat = useChatUiStore((s) => s.closeChat);
  const chatOpen = useChatUiStore((s) => s.chatOpen);

  const openOnlinePanel = () => {
    if (chatOpen) {
      closeChat();
    }
    setSidebarTab("online");
    if (fullscreen) {
      void setFullscreen(false);
    }
  };

  return (
    <Button
      type="button"
      variant="secondary"
      size="icon" className="size-8"
      onClick={openOnlinePanel}
      aria-label="打开在线视频"
    >
      <Link2 className="size-4" />
    </Button>
  );
}
