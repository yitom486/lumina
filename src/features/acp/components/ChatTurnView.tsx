import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { cn } from "@/lib/utils";
import { errorMessage, formatTime } from "@/lib/format";

import { AnnotationProposalCard } from "@/features/notes/components/AnnotationProposalCard";
import { createNote } from "@/features/notes/api";
import { usePlayerStore } from "@/features/player";

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

/**
 * P6-M3: save any done answer as a note (two-step inline confirm, no dialog
 * over video). Reuses the confirm-before-write chain; QuickNoteDialog keeps
 * its live-position semantics untouched.
 *
 * QueryClient is only touched inside the mounted confirm step, so turns
 * rendered without a query context (tests, history previews) keep working.
 */
function SaveAnswerNote({ turn }: { turn: ChatTurn }) {
  const [confirming, setConfirming] = useState(false);
  const [saved, setSaved] = useState(false);

  if (turn.status !== "done" || !turn.answer.trim()) return null;
  if (saved) {
    return (
      <p className="mt-2 text-[11px] text-emerald-600 dark:text-emerald-400">
        已保存为批注
      </p>
    );
  }
  if (!confirming) {
    return (
      <button
        type="button"
        className="mt-2 text-[11px] text-muted-foreground hover:text-foreground"
        onClick={() => setConfirming(true)}
      >
        存为批注
      </button>
    );
  }
  return (
    <ConfirmSaveNote
      turn={turn}
      onSaved={() => {
        setConfirming(false);
        setSaved(true);
      }}
      onCancel={() => setConfirming(false)}
    />
  );
}

function ConfirmSaveNote({
  turn,
  onSaved,
  onCancel,
}: {
  turn: ChatTurn;
  onSaved: () => void;
  onCancel: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const queryClient = useQueryClient();
  const store = usePlayerStore.getState();
  const anchorMs = turn.anchorMs ?? store.currentTimeMs;

  return (
    <span className="mt-2 inline-flex flex-wrap items-center gap-1 text-[11px]">
      <span className="text-muted-foreground">
        保存到 {formatTime(anchorMs)} 的批注？
      </span>
      <button
        type="button"
        className="font-medium text-sky-400 hover:text-sky-300 disabled:opacity-50"
        disabled={busy}
        onClick={() => {
          void (async () => {
            const live = usePlayerStore.getState();
            const mediaPath = live.currentFile;
            if (!mediaPath) {
              setError("请先打开视频");
              return;
            }
            setBusy(true);
            setError(null);
            try {
              await createNote({
                mediaPath,
                positionMs: turn.anchorMs ?? live.currentTimeMs,
                body: turn.answer.trim(),
                includeQuotes: false,
              });
              await queryClient.invalidateQueries({
                queryKey: ["notes", mediaPath],
              });
              onSaved();
            } catch (cause) {
              setError(errorMessage(cause));
            } finally {
              setBusy(false);
            }
          })();
        }}
      >
        {busy ? "保存中…" : "保存"}
      </button>
      <button
        type="button"
        className="text-muted-foreground hover:text-foreground"
        disabled={busy}
        onClick={onCancel}
      >
        取消
      </button>
      {error ? <span className="text-destructive">{error}</span> : null}
    </span>
  );
}

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

        <SaveAnswerNote turn={turn} />

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
