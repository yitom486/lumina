import { cn } from "@/lib/utils";

import type { ChatTurn } from "../types";
import { ChatActivityFeed } from "./ChatActivityFeed";
import { ChatColumn } from "./ChatShell";

type Props = {
  turn: ChatTurn;
};

export function ChatTurnView({ turn }: Props) {
  const isError = turn.status === "error";
  const isStreaming = turn.status === "streaming";

  return (
    <ChatColumn className="space-y-2">
      <div className="flex justify-end">
        <div className="max-w-[88%] rounded-lg bg-accent px-3 py-2 text-sm text-accent-foreground whitespace-pre-wrap break-words">
          {turn.userText}
        </div>
      </div>

      <div className="w-full">
        {turn.showActivities && turn.activities.length > 0 ? (
          <ChatActivityFeed activities={turn.activities} streaming={isStreaming} />
        ) : null}

        <div
          className={cn(
            "w-full rounded-lg px-3 py-2 text-sm whitespace-pre-wrap break-words",
            !isError && "bg-muted/40 text-foreground",
            isError && "bg-destructive/10 text-destructive",
          )}
        >
          {turn.answer || (isStreaming ? "…" : "")}
          {isStreaming ? (
            <span className="ml-0.5 inline-block animate-pulse text-muted-foreground">
              ▍
            </span>
          ) : null}
        </div>
      </div>
    </ChatColumn>
  );
}
