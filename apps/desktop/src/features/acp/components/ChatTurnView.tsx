import { useQueryClient } from "@tanstack/react-query";
import { Suspense, lazy, useState } from "react";

import { cn } from "@lumina/ui/utils";
import { errorMessage, formatTime } from "@/lib/format";

import { AnnotationProposalCard } from "@/features/notes/components/AnnotationProposalCard";
import { createNote } from "@/features/notes/api";
import { notesKey } from "@lumina/query-keys";
import { usePlayerStore } from "@/features/player";

import { waitingLabel, hasActiveToolActivity } from "@lumina/chat-ui/activityStatus";
import {
  presentChatActivities,
  type ChatPresentationMode,
} from "@lumina/chat-ui/presentationPolicy";
import type { AssistantAction } from "@lumina/chat-ui/assistantBlocks";
import { parseAssistantBlocksText } from "@lumina/chat-ui/assistantBlocks";
import { RichBlockRenderer } from "@lumina/chat-ui/components/RichBlockRenderer";
import type { ChatTurn } from "../types";
import { normalizeRestoredShortcutOutput } from "../shortcutOutput";
import { ChatActivityFeed } from "@lumina/chat-ui/components/ChatActivityFeed";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";
import { ChatWaitingDots } from "@lumina/chat-ui/components/ChatWaitingDots";

// Split markdown+KaTeX out of the initial bundle: chat is never visible on
// cold start, so parsing ~400KB can wait until the first answer renders.
const ChatMarkdown = lazy(() =>
  import("./ChatMarkdown").then((module) => ({ default: module.ChatMarkdown })),
);

type Props = {
  turn: ChatTurn;
  annotationWorkspace?: string | null;
  onDismissAnnotation?: (turnId: string) => void;
  onSaveAnnotation?: (turnId: string, proposalId?: string) => void;
  onAssistantAction?: (action: AssistantAction) => void;
  /** Tests may choose a mode; production derives it once from the Vite build. */
  presentationMode?: ChatPresentationMode;
};

const BUILD_PRESENTATION_MODE: ChatPresentationMode = import.meta.env.DEV
  ? "development"
  : "release";

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
      <p className="mt-2 text-[11px] text-success">
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
        className="font-medium text-info hover:text-info-foreground disabled:opacity-50"
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
                queryKey: notesKey(mediaPath),
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
  onAssistantAction,
  presentationMode = BUILD_PRESENTATION_MODE,
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
  const visibleActivities = presentChatActivities(
    turn.activities,
    presentationMode,
  );
  const toolCount = visibleActivities.filter((item) => item.kind === "tool").length;
  const showActivityFeed =
    visibleActivities.length > 0 &&
    (isStreaming || (presentationMode === "development" && toolsOpen));
  const restoredShortcut =
    !isError && !isStreaming && visibleAnswer
      ? normalizeRestoredShortcutOutput(visibleAnswer)
      : null;
  const normalizedAnswer = restoredShortcut?.answer ?? visibleAnswer;
  const structuredAnswer =
    !isError && normalizedAnswer
      ? parseAssistantBlocksText(normalizedAnswer, { streaming: isStreaming })
      : null;
  const richBlocks = structuredAnswer?.blocks ?? [];
  const answerText =
    (looksLikeStructuredJson(normalizedAnswer) && !richBlocks.length
      ? "正在整理结构化结果…"
      : normalizedAnswer);
  const releaseLiveLabel =
    presentationMode === "release"
      ? visibleActivities.find((item) => item.kind === "tool")?.title
      : undefined;

  return (
    <ChatColumn className="space-y-2">
      <div className="flex justify-end">
        <div className="max-w-[88%] space-y-2 rounded-lg bg-accent px-3 py-2 text-sm text-accent-foreground whitespace-pre-wrap break-words">
          {turn.images && turn.images.length > 0 ? (
            <div className="flex flex-wrap justify-end gap-2">
              {turn.images.map((image) => (
                <img
                  key={image.id}
                  src={image.dataUrl}
                  alt="发送的图片"
                  className="max-h-40 rounded-md border border-border/50 object-contain"
                />
              ))}
            </div>
          ) : null}
          {turn.userText}
        </div>
      </div>

      <div className="w-full">
        {presentationMode === "development" &&
        visibleActivities.length > 0 &&
        !showActivityFeed ? (
          <button
            type="button"
            className="mb-2 text-[11px] text-muted-foreground hover:text-foreground"
            onClick={() => setToolsOpen(true)}
          >
            {toolCount > 0
              ? `查看本轮调试记录（${toolCount} 个工具）`
              : "查看本轮调试记录"}
          </button>
        ) : null}

        {showActivityFeed ? (
          <ChatActivityFeed
            activities={visibleActivities}
            streaming={isStreaming}
            collapsible={!isStreaming}
            liveLabel={releaseLiveLabel}
            onRequestCollapse={
              !isStreaming ? () => setToolsOpen(false) : undefined
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
          {answerText ? (
            isError ? (
              answerText
            ) : richBlocks.length ? (
              <Suspense
                fallback={
                  <span className="whitespace-pre-wrap">{answerText}</span>
                }
              >
                <RichBlockRenderer
                  blocks={richBlocks}
                  onAction={onAssistantAction}
                  renderMarkdown={(markdown) => <ChatMarkdown content={markdown} />}
                />
              </Suspense>
            ) : (
              <Suspense
                fallback={
                  <span className="whitespace-pre-wrap">{answerText}</span>
                }
              >
                <ChatMarkdown content={answerText} />
              </Suspense>
            )
          ) : showWaitingDots ? (
            <ChatWaitingDots label={waitingLabel(turn.activities)} />
          ) : waitingForText ? null : (
            ""
          )}
          {answerText && isStreaming ? (
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
          <div className="mt-2 rounded-lg border border-success/30 bg-success/10 px-3 py-2 text-[12px] text-success-foreground">
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

function looksLikeStructuredJson(value: string): boolean {
  const source = value.trimStart();
  return source.startsWith("{") || source.startsWith("[") || /^```json\s/i.test(source);
}
