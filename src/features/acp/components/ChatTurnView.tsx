import { cn } from "@/lib/utils";

import { waitingLabel } from "../activityStatus";
import type { ChatTurn } from "../types";
import { ChatActivityFeed } from "./ChatActivityFeed";
import { ChatMarkdown } from "./ChatMarkdown";
import { ChatColumn } from "./ChatShell";
import { ChatWaitingDots } from "./ChatWaitingDots";

type Props = {
  turn: ChatTurn;
};

export function ChatTurnView({ turn }: Props) {
  const isError = turn.status === "error";
  const isStreaming = turn.status === "streaming";
  const waitingForText = isStreaming && !turn.answer.trim();
  const showWaitingDots =
    waitingForText && !(turn.showActivities && turn.activities.length > 0);

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
            "w-full rounded-lg px-3 py-2 text-sm break-words",
            !isError && "bg-muted/40 text-foreground",
            isError && "bg-destructive/10 text-destructive whitespace-pre-wrap",
            isStreaming && !isError && "chat-reply-streaming",
          )}
          data-turn-id={turn.id}
        >
          {turn.answer ? (
            isError ? (
              turn.answer
            ) : (
              <ChatMarkdown content={turn.answer} />
            )
          ) : showWaitingDots ? (
            <ChatWaitingDots label={waitingLabel(turn.activities)} />
          ) : waitingForText ? null : (
            ""
          )}
          {turn.answer && isStreaming ? (
            <span className="chat-stream-caret ml-0.5 inline-block text-muted-foreground">
              ▍
            </span>
          ) : null}
        </div>
        {isError && turn.errorHint ? (
          <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
            {turn.errorHint}
          </p>
        ) : null}
      </div>
    </ChatColumn>
  );
}
