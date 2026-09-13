import { MessageSquare, Sparkles } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

import { useChatUiStore } from "../chatUiStore";

/** Top-right entry — opens the parallel ChatDock (not a sidebar tab). */
export function ChatToggleButton({ className }: { className?: string }) {
  const chatOpen = useChatUiStore((s) => s.chatOpen);
  const toggleChat = useChatUiStore((s) => s.toggleChat);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant={chatOpen ? "secondary" : "ghost"}
          size="icon-sm"
          className={cn("relative gap-0.5 px-2", className)}
          aria-label={chatOpen ? "收起 AI 对话" : "打开 AI 对话"}
          aria-pressed={chatOpen}
          onClick={toggleChat}
        >
          <Sparkles className="size-3.5 text-amber-400/90" />
          <MessageSquare className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {chatOpen ? "收起对话 (Esc)" : "AI 对话"}
      </TooltipContent>
    </Tooltip>
  );
}
