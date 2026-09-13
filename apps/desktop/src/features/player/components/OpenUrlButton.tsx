/** Control-bar shortcut: jump to sidebar「在线」tab (never open dialog over HWND). */

import { Link2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useChatUiStore } from "@/features/acp/chatUiStore";

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
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="secondary"
          size="icon-sm"
          onClick={openOnlinePanel}
          aria-label="打开在线视频"
        >
          <Link2 className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>在线视频（侧栏打开，避免被画面挡住）</TooltipContent>
    </Tooltip>
  );
}
