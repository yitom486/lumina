import { useQuery } from "@tanstack/react-query";
import { ChevronDown, Clock3, Sparkles } from "lucide-react";
import { useEffect, useId, useState } from "react";

import {
  parseAssistantBlocksText,
  type AssistantAction,
} from "@lumina/chat-ui/assistantBlocks";
import { ChatColumn } from "@lumina/chat-ui/components/ChatShell";
import { RichBlockRenderer } from "@lumina/chat-ui/components/RichBlockRenderer";
import { usePlayerStore } from "@/features/player";
import {
  acpWatchFeedQueryKey,
  getAcpWatchFeed,
  type AcpTaskId,
  type AcpWatchFeedItem,
} from "../api";
import {
  adaptShortcutOutput,
  normalizeRestoredShortcutOutput,
} from "../shortcutOutput";
import type { CompanionTaskId } from "./CompanionQuickActions";
import { CompanionQuickActions } from "./CompanionQuickActions";
import { ChatMarkdown } from "./ChatMarkdown";
import {
  countWatchFeedItemsAfterPosition,
  currentWatchFeedChapter,
  selectWatchFeedSlots,
  type WatchFeedSlot,
} from "./watchFeedProjection";

type Props = {
  onSelectTask: (taskId: CompanionTaskId) => void;
  quickActionsDisabled?: boolean;
  onAssistantAction?: (action: AssistantAction) => void;
};

const SLOT_ORDER: readonly WatchFeedSlot[] = [
  "watch-record",
  "recap",
  "highlights",
];

/**
 * A compact projection of the durable watch-feed state.
 *
 * SQLite keeps every published item, but this surface intentionally renders
 * one replaceable card per semantic slot. The conversation below this view is
 * rendered by ChatTurnList and remains the single shared ACP chat session.
 */
export function WatchFeedView({
  onSelectTask,
  quickActionsDisabled,
  onAssistantAction,
}: Props) {
  const currentFile = usePlayerStore((state) => state.currentFile);
  const currentTimeMs = usePlayerStore((state) => state.currentTimeMs);
  const watchFeedQuery = useQuery({
    queryKey: [...acpWatchFeedQueryKey, currentFile ?? null],
    queryFn: getAcpWatchFeed,
    enabled: Boolean(currentFile),
    retry: false,
    staleTime: 1_000,
  });
  const persistedItems =
    watchFeedQuery.data?.source === "sqlite" ? watchFeedQuery.data.items : [];
  const slots = selectWatchFeedSlots(persistedItems, currentTimeMs);
  const futureItemCount = countWatchFeedItemsAfterPosition(
    persistedItems,
    currentTimeMs,
  );
  const currentChapter = currentWatchFeedChapter(persistedItems, currentTimeMs);
  const hasVisibleSlot = SLOT_ORDER.some((slot) => slots[slot] !== null);

  return (
    <ChatColumn className="space-y-2 py-2">
      <div id="companion-panel-watch-feed" role="region" aria-label="AI 观剧流">
        <WatchFeedQueryStatus
          isLoading={watchFeedQuery.isLoading}
          isError={watchFeedQuery.isError}
          source={watchFeedQuery.data?.source}
        />
        {persistedItems.length > 0 ? (
          <WatchFeedPositionStatus
            currentTimeMs={currentTimeMs}
            currentChapterTitle={currentChapter?.title ?? null}
            futureItemCount={futureItemCount}
          />
        ) : null}

        {hasVisibleSlot ? (
          <div
            className="space-y-1.5"
            aria-label="当前观剧上下文"
            data-watch-feed-slots
          >
            {SLOT_ORDER.map((slot) => {
              const item = slots[slot];
              return item ? (
                <WatchFeedSlotCard
                  key={slot}
                  slot={slot}
                  item={item}
                  defaultExpanded={slot === "watch-record"}
                  onAssistantAction={onAssistantAction}
                />
              ) : null;
            })}
          </div>
        ) : !watchFeedQuery.isLoading && futureItemCount > 0 ? (
          <FutureWatchFeedNotice />
        ) : !watchFeedQuery.isLoading && !watchFeedQuery.isError ? (
          <EmptyWatchFeed />
        ) : null}

        <div className="border-t border-border/70 pt-2">
          <CompanionQuickActions
            disabled={quickActionsDisabled}
            onSelectTask={onSelectTask}
          />
        </div>
      </div>
    </ChatColumn>
  );
}

function WatchFeedSlotCard({
  slot,
  item,
  defaultExpanded,
  onAssistantAction,
}: {
  slot: WatchFeedSlot;
  item: AcpWatchFeedItem;
  defaultExpanded: boolean;
  onAssistantAction?: (action: AssistantAction) => void;
}) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const contentId = useId();

  // A new chapter is a new state snapshot. Make the current record visible,
  // while keeping the user's explicit collapse choice during stable renders.
  useEffect(() => {
    setExpanded(defaultExpanded);
  }, [defaultExpanded, item.id]);

  return (
    <section
      className="overflow-hidden rounded-lg border border-border/80 bg-card/70"
      data-watch-feed-slot={slot}
      data-watch-feed-item-id={item.id}
    >
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
        aria-expanded={expanded}
        aria-controls={contentId}
        onClick={() => setExpanded((value) => !value)}
      >
        <Sparkles className="size-3.5 shrink-0 text-primary" aria-hidden />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-xs font-medium text-foreground">
            {slotLabel(slot)}
          </span>
          <span className="block truncate text-[10px] text-muted-foreground">
            {slotHint(slot)}
          </span>
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground">
          {spoilerLevelLabel(item.spoilerLevel)}
        </span>
        <ChevronDown
          className={`size-3.5 shrink-0 text-muted-foreground transition-transform ${
            expanded ? "rotate-180" : ""
          }`}
          aria-hidden
        />
      </button>

      {expanded ? (
        <div
          id={contentId}
          className="max-h-48 overflow-y-auto border-t border-border/70 px-3 py-2 text-xs"
        >
          <div className="mb-1.5 flex items-center gap-1.5 text-[10px] text-muted-foreground">
            <Clock3 className="size-3" aria-hidden />
            <span>{formatItemRange(item)}</span>
          </div>
          <PersistedWatchFeedContent
            item={item}
            onAssistantAction={onAssistantAction}
          />
          {item.screenshotRefs.length > 0 || item.coverRef ? (
            <p className="mt-2 border-t border-border/70 pt-2 text-[10px] text-muted-foreground">
              {item.coverRef ? "含章节封面引用" : null}
              {item.screenshotRefs.length > 0
                ? `${item.coverRef ? " · " : ""}含 ${item.screenshotRefs.length} 个画面引用`
                : null}
            </p>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

function PersistedWatchFeedContent({
  item,
  onAssistantAction,
}: {
  item: AcpWatchFeedItem;
  onAssistantAction?: (action: AssistantAction) => void;
}) {
  const restored = normalizeRestoredShortcutOutput(item.content);
  const taskId = restored.shortcutTaskId ?? shortcutTaskForItem(item.itemType);
  const shortcut = taskId ? adaptShortcutOutput(taskId, item.content) : null;
  const structured = shortcut?.blocks.length
    ? shortcut
    : parseAssistantBlocksText(item.content);

  if (shortcut?.blocks.length) {
    return (
      <RichBlockRenderer
        blocks={shortcut.blocks}
        density="compact"
        onAction={onAssistantAction}
        renderMarkdown={(markdown) => <ChatMarkdown content={markdown} />}
      />
    );
  }

  if (shortcut?.fallbackText) {
    return <p className="text-xs leading-relaxed text-muted-foreground">{shortcut.fallbackText}</p>;
  }

  if (structured?.blocks.length) {
    return (
      <RichBlockRenderer
        blocks={structured.blocks}
        density="compact"
        onAction={onAssistantAction}
        renderMarkdown={(markdown) => <ChatMarkdown content={markdown} />}
      />
    );
  }

  if (looksLikeJson(item.content)) {
    return (
      <p className="text-xs leading-relaxed text-muted-foreground">
        该结构化内容暂时无法展示，请稍后重试。
      </p>
    );
  }

  return <ChatMarkdown content={item.content} />;
}

function shortcutTaskForItem(itemType: string): AcpTaskId | null {
  switch (itemType.trim().toLowerCase()) {
    case "recap":
    case "chapter_recap":
      return "chapter_recap";
    case "outlook":
    case "chapter_outlook":
      return "chapter_outlook";
    case "question":
    case "question_candidates":
      return "question_candidates";
    case "plot_summary":
    case "summary":
      return "plot_summary";
    default:
      return null;
  }
}

function looksLikeJson(value: string): boolean {
  const source = value.trim();
  return (
    (source.startsWith("{") && source.endsWith("}")) ||
    (source.startsWith("[") && source.endsWith("]")) ||
    /^```json\s/i.test(source)
  );
}

function slotLabel(slot: WatchFeedSlot): string {
  switch (slot) {
    case "watch-record":
      return "观剧记录";
    case "recap":
      return "前情提要";
    case "highlights":
      return "本段要点";
  }
}

function slotHint(slot: WatchFeedSlot): string {
  switch (slot) {
    case "watch-record":
      return "当前播放位置的观察";
    case "recap":
      return "只回顾已经看过的内容";
    case "highlights":
      return "不剧透的关注点和问题";
  }
}

function formatItemRange(item: AcpWatchFeedItem): string {
  if (!item.chapter) return "当前播放位置";
  return `${item.chapter.title || "当前章节"} · ${formatTime(item.chapter.startMs)}${
    item.chapter.endMs > item.chapter.startMs
      ? `–${formatTime(item.chapter.endMs)}`
      : ""
  }`;
}

function spoilerLevelLabel(level: string): string {
  switch (level) {
    case "current_position":
      return "当前进度";
    case "current_chapter":
      return "当前章节";
    case "none":
      return "不剧透";
    default:
      return "已标注范围";
  }
}

function WatchFeedPositionStatus({
  currentTimeMs,
  currentChapterTitle,
  futureItemCount,
}: {
  currentTimeMs: number;
  currentChapterTitle: string | null;
  futureItemCount: number;
}) {
  return (
    <div
      className="flex flex-wrap items-center gap-x-2 gap-y-1 px-1 pb-1 text-[10px] text-muted-foreground"
      data-watch-feed-position={currentTimeMs}
      role="status"
    >
      <span>当前播放 · {formatTime(currentTimeMs)}</span>
      {currentChapterTitle ? <span>· {currentChapterTitle}</span> : null}
      {futureItemCount > 0 ? (
        <span>· {futureItemCount} 条后续内容将在播放到对应位置后显示</span>
      ) : null}
    </div>
  );
}

function WatchFeedQueryStatus({
  isLoading,
  isError,
  source,
}: {
  isLoading: boolean;
  isError: boolean;
  source?: "sqlite" | "empty";
}) {
  if (isLoading) {
    return (
      <p className="px-1 py-1 text-[11px] text-muted-foreground" role="status">
        正在读取已保存的观剧流…
      </p>
    );
  }

  if (isError) {
    return (
      <p className="rounded-lg border border-border bg-muted/20 px-3 py-2 text-[11px] text-muted-foreground">
        本地观剧流暂时不可用，聊天仍可继续使用。
      </p>
    );
  }

  if (source === "empty") return null;
  return null;
}

function FutureWatchFeedNotice() {
  return (
    <div className="rounded-lg border border-dashed border-border px-3 py-4 text-center text-[11px] text-muted-foreground">
      观剧流已同步到当前进度，后续内容将在播放到对应位置后显示。
    </div>
  );
}

function EmptyWatchFeed() {
  return (
    <div className="rounded-lg border border-dashed border-border px-3 py-4 text-center">
      <Sparkles className="mx-auto size-4 text-muted-foreground" aria-hidden />
      <p className="mt-2 text-xs font-medium text-foreground">AI 观剧流已就绪</p>
      <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
        播放到有分析结果的内容后，当前上下文会显示在这里。
      </p>
    </div>
  );
}

function formatTime(milliseconds: number): string {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
