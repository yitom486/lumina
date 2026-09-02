import { useState } from "react";

import { cn } from "@/lib/utils";

import { AnnotationProposalCard } from "@/features/notes/components/AnnotationProposalCard";

import { waitingLabel, hasActiveToolActivity } from "../activityStatus";
import type { ChatTurn } from "../types";
import { ChatActivityFeed } from "./ChatActivityFeed";
import { ChatMarkdown } from "./ChatMarkdown";
import { ChatColumn } from "./ChatShell";
import { ChatWaitingDots } from "./ChatWaitingDots";

type Props = {
  turn: ChatTurn;
  annotationWorkspace?: string | null;
  onDismissAnnotation?: (turnId: string) => void;
  onSaveAnnotation?: (turnId: string, proposalId?: string) => void;
};

export function ChatTurnView({
  turn,
  annotationWorkspace,
  onDismissAnnotation,
  onSaveAnnotation,
}: Props) {
  const [toolsOpen, setToolsOpen] = useState(false);
  const isError = turn.status === "error";
  const isStreaming = turn.status === "streaming";
  const hideAnswerWhileTools =
    isStreaming && hasActiveToolActivity(turn.activities);
  const visibleAnswer = hideAnswerWhileTools ? "" : turn.answer;
  const waitingForText = isStreaming && !visibleAnswer.trim();
  const showWaitingDots =
    waitingForText && !(turn.showActivities && turn.activities.length > 0);
  const toolCount = turn.activities.filter((item) => item.kind === "tool").length;
  const showActivityFeed =
    turn.activities.length > 0 && (turn.showActivities || toolsOpen);

  return (
    <ChatColumn className="space-y-2">
      <div className="flex justify-end">
        <div className="max-w-[88%] rounded-lg bg-accent px-3 py-2 text-sm text-accent-foreground whitespace-pre-wrap break-words">
          {turn.userText}
        </div>
      </div>

      <div className="w-full">
        {toolCount > 0 && !showActivityFeed ? (
          <button
            type="button"
            className="mb-2 text-[11px] text-muted-foreground hover:text-foreground"
            onClick={() => setToolsOpen(true)}
          >
            查看工具执行（{toolCount}）
          </button>
        ) : null}

        {showActivityFeed ? (
          <ChatActivityFeed
            activities={turn.activities}
            streaming={isStreaming}
            collapsible={!isStreaming && !turn.showActivities}
            onRequestCollapse={
              !turn.showActivities ? () => setToolsOpen(false) : undefined
            }
          />
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
          {visibleAnswer ? (
            isError ? (
              visibleAnswer
            ) : (
              <ChatMarkdown content={visibleAnswer} />
            )
          ) : showWaitingDots ? (
            <ChatWaitingDots label={waitingLabel(turn.activities)} />
          ) : waitingForText ? null : (
            ""
          )}
          {visibleAnswer && isStreaming ? (
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

        {turn.annotationProposalSaved ? (
          <div className="mt-2 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-[12px] text-emerald-800 dark:text-emerald-300">
            批注已写入笔记库
          </div>
        ) : turn.annotationProposal && annotationWorkspace ? (
          <AnnotationProposalCard
            className="mt-2"
            proposal={turn.annotationProposal}
            workspace={annotationWorkspace}
            onDismiss={() => onDismissAnnotation?.(turn.id)}
            onSaved={() =>
              onSaveAnnotation?.(
                turn.id,
                turn.annotationProposal?.proposalId,
              )
            }
          />
        ) : null}
      </div>
    </ChatColumn>
  );
}
