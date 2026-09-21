import { useQuery } from "@tanstack/react-query";
import { Activity, Clock3, MessageSquareText, Sparkles } from "lucide-react";
import { useEffect, useRef } from "react";

import {
  parseAssistantBlocksText,
  type AssistantAction,
} from "@lumina/chat-ui/assistantBlocks";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";
import { RichBlockRenderer } from "@lumina/chat-ui/components/RichBlockRenderer";
import { hasActiveToolActivity } from "@lumina/chat-ui/activityStatus";
import { isToolFailed, isToolSucceeded } from "@lumina/chat-ui/toolStatus";
import { usePlayerStore } from "@/features/player";
import {
  acpWatchFeedQueryKey,
  getAcpWatchFeed,
  type AcpWatchFeedItem,
} from "../api";
import type { ChatTurn } from "../types";
import type { CompanionTaskId } from "./CompanionQuickActions";
import { CompanionQuickActions } from "./CompanionQuickActions";
import { ChatMarkdown } from "./ChatMarkdown";
import { ChatTurnView } from "./ChatTurnView";

type Props = {
  turns: ChatTurn[];
  notices: { id: string; content: string }[];
  followEnd: boolean;
  annotationWorkspace?: string | null;
  onDismissAnnotation?: (turnId: string) => void;
  onSaveAnnotation?: (turnId: string) => void;
  onSelectTask: (taskId: CompanionTaskId) => void;
  quickActionsDisabled?: boolean;
  onAssistantAction?: (action: AssistantAction) => void;
};

/**
 * A watch-oriented projection of the same real ACP turns.
 * It deliberately does not create a second session or write to chat history.
 */
export function WatchFeedView({
  turns,
  notices,
  followEnd,
  annotationWorkspace,
  onDismissAnnotation,
  onSaveAnnotation,
  onSelectTask,
  quickActionsDisabled,
  onAssistantAction,
}: Props) {
  const endRef = useRef<HTMLDivElement>(null);
  const currentFile = usePlayerStore((state) => state.currentFile);
  const watchFeedQuery = useQuery({
    queryKey: [...acpWatchFeedQueryKey, currentFile ?? null],
    queryFn: getAcpWatchFeed,
    enabled: Boolean(currentFile),
    retry: false,
    staleTime: 1_000,
  });
  const persistedItems =
    watchFeedQuery.data?.source === "sqlite" ? watchFeedQuery.data.items : [];
  const hasFallbackContent = turns.length > 0 || notices.length > 0;
  const showingFallback = persistedItems.length === 0;

  useEffect(() => {
    if (followEnd) {
      endRef.current?.scrollIntoView({ behavior: "auto", block: "end" });
    }
  }, [followEnd, notices, persistedItems, turns]);

  return (
    <ChatColumn className="space-y-3 py-3">
      <div id="companion-panel-watch-feed" role="tabpanel" aria-label="AI 观剧流">
        <WatchFeedQueryStatus
          isLoading={watchFeedQuery.isLoading}
          isError={watchFeedQuery.isError}
          source={watchFeedQuery.data?.source}
          hasFallbackContent={hasFallbackContent}
        />
        {!watchFeedQuery.isLoading && showingFallback && !hasFallbackContent ? (
          <EmptyWatchFeed />
        ) : persistedItems.length > 0 ? (
          persistedItems.map((item) => (
            <PersistedWatchFeedItem
              key={`sqlite-${item.id}`}
              item={item}
              onAssistantAction={onAssistantAction}
            />
          ))
        ) : (
          <>
            {turns.map((turn) => (
              <article
                key={turn.id}
                className="overflow-hidden rounded-xl border border-border bg-card shadow-sm"
              >
                <header className="flex items-center justify-between gap-2 border-b border-border/70 px-3 py-2">
                  <span className="inline-flex min-w-0 items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
                    <MessageSquareText className="size-3.5 shrink-0" aria-hidden />
                    <span className="truncate">AI 观剧记录</span>
                  </span>
                  {typeof turn.anchorMs === "number" ? (
                    <span className="inline-flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground">
                      <Clock3 className="size-3" aria-hidden />
                      {formatTime(turn.anchorMs)}
                    </span>
                  ) : null}
                  <ToolActivitySummary turn={turn} />
                </header>
                <div className="px-3">
                  <ChatTurnView
                    turn={turn}
                    annotationWorkspace={annotationWorkspace}
                    onDismissAnnotation={onDismissAnnotation}
                    onSaveAnnotation={onSaveAnnotation}
                    onAssistantAction={onAssistantAction}
                  />
                </div>
              </article>
            ))}
            {notices.map((notice) => (
              <p key={notice.id} className="text-center text-[11px] text-muted-foreground">
                {notice.content}
              </p>
            ))}
          </>
        )}
        <div className="border-t border-border/70 pt-2">
          <CompanionQuickActions
            disabled={quickActionsDisabled}
            onSelectTask={onSelectTask}
          />
        </div>
        <div ref={endRef} aria-hidden />
      </div>
    </ChatColumn>
  );
}

function WatchFeedQueryStatus({
  isLoading,
  isError,
  source,
  hasFallbackContent,
}: {
  isLoading: boolean;
  isError: boolean;
  source?: "sqlite" | "empty";
  hasFallbackContent: boolean;
}) {
  if (isLoading && !hasFallbackContent) {
    return (
      <p className="px-3 py-2 text-center text-xs text-muted-foreground" role="status">
        正在读取已保存的观剧流…
      </p>
    );
  }

  if (isError) {
    return (
      <p className="rounded-lg border border-border bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
        本地观剧流暂时不可用，当前显示本次会话的临时记录。
      </p>
    );
  }

  if (source === "empty" && !hasFallbackContent) {
    return (
      <p className="rounded-lg border border-border bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
        暂无已保存的观剧条目；本次会话结果会安全显示在这里。
      </p>
    );
  }

  return null;
}

function PersistedWatchFeedItem({
  item,
  onAssistantAction,
}: {
  item: AcpWatchFeedItem;
  onAssistantAction?: (action: AssistantAction) => void;
}) {
  const structured = parseAssistantBlocksText(item.content);

  return (
    <article
      className="mb-3 overflow-hidden rounded-xl border border-border bg-card shadow-sm"
      data-watch-feed-source="sqlite"
      data-watch-feed-item-type={item.itemType}
    >
      <header className="flex items-center justify-between gap-2 border-b border-border/70 px-3 py-2">
        <span className="inline-flex min-w-0 items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
          <Sparkles className="size-3.5 shrink-0" aria-hidden />
          <span className="truncate">{watchFeedItemLabel(item.itemType)}</span>
        </span>
        <span
          className="shrink-0 text-[10px] text-muted-foreground"
          data-spoiler-level={item.spoilerLevel}
        >
          {spoilerLevelLabel(item.spoilerLevel)}
        </span>
      </header>
      {item.chapter ? (
        <div className="flex items-center gap-1.5 px-3 pt-2 text-[10px] text-muted-foreground">
          <Clock3 className="size-3" aria-hidden />
          <span>
            {item.chapter.title || "章节"} · {formatTime(item.chapter.startMs)}
            {item.chapter.endMs > item.chapter.startMs
              ? `–${formatTime(item.chapter.endMs)}`
              : ""}
          </span>
        </div>
      ) : null}
      <div className="px-3 py-2">
        {structured?.blocks.length ? (
          <RichBlockRenderer
            blocks={structured.blocks}
            onAction={onAssistantAction}
            renderMarkdown={(markdown) => <ChatMarkdown content={markdown} />}
          />
        ) : (
          <ChatMarkdown content={item.content} />
        )}
      </div>
      {item.screenshotRefs.length > 0 || item.coverRef ? (
        <footer className="flex items-center gap-2 border-t border-border/70 px-3 py-2 text-[10px] text-muted-foreground">
          {item.coverRef ? "含章节封面引用" : null}
          {item.screenshotRefs.length > 0
            ? `含 ${item.screenshotRefs.length} 个画面引用`
            : null}
        </footer>
      ) : null}
    </article>
  );
}

function watchFeedItemLabel(itemType: string): string {
  switch (itemType) {
    case "chapter":
      return "章节主线";
    case "recap":
      return "前情提要";
    case "outlook":
      return "后续看点";
    case "question":
      return "观众问题";
    case "watch_point":
      return "观剧看点";
    default:
      return "AI 观剧记录";
  }
}

function spoilerLevelLabel(level: string): string {
  switch (level) {
    case "current_position":
      return "当前进度";
    case "current_chapter":
      return "当前章节";
    case "full_media":
      return "全片分析";
    default:
      return "已标注剧透范围";
  }
}

function EmptyWatchFeed() {
  return (
    <div className="flex min-h-48 flex-col items-center justify-center rounded-xl border border-dashed border-border px-6 py-8 text-center">
      <SparkleMark />
      <p className="mt-3 text-sm font-medium text-foreground">AI 观剧流已就绪</p>
      <p className="mt-1 max-w-xs text-xs leading-relaxed text-muted-foreground">
        从当前播放位置提问后，回答、台词引用和工具活动会按观看锚点整理在这里。
      </p>
    </div>
  );
}

function SparkleMark() {
  return (
    <span
      className="flex size-9 items-center justify-center rounded-full bg-accent text-accent-foreground"
      aria-hidden
    >
      <Sparkles className="size-4" />
    </span>
  );
}

function ToolActivitySummary({ turn }: { turn: ChatTurn }) {
  const toolActivities = turn.activities.filter((item) => item.kind === "tool");
  if (toolActivities.length === 0) return null;

  const status = hasActiveToolActivity(turn.activities)
    ? "执行中"
    : toolActivities.some((item) => isToolFailed(item.status))
      ? "有失败"
      : toolActivities.every((item) => isToolSucceeded(item.status))
        ? "已完成"
        : "进行中";

  return (
    <span className="inline-flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground">
      <Activity className="size-3" aria-hidden />
      {toolActivities.length} 个工具活动 · {status}
    </span>
  );
}

function formatTime(milliseconds: number): string {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
