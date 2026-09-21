import type { AcpWatchFeedItem } from "../api";

export type WatchFeedSlot = "watch-record" | "recap" | "highlights";

export type WatchFeedSlots = Readonly<{
  "watch-record": AcpWatchFeedItem | null;
  recap: AcpWatchFeedItem | null;
  highlights: AcpWatchFeedItem | null;
}>;

/**
 * Keep the durable feed as the source of truth, but do not reveal a future
 * chapter while the viewer is still before its start anchor. Items without a
 * chapter anchor are session-level records and remain visible.
 */
export function selectWatchFeedItemsForPosition(
  items: AcpWatchFeedItem[],
  positionMs: number,
): AcpWatchFeedItem[] {
  const safePositionMs = Number.isFinite(positionMs) ? Math.max(0, positionMs) : 0;
  return items.filter((item) => {
    const startMs = item.chapter?.startMs;
    return typeof startMs !== "number" || startMs <= safePositionMs;
  });
}

export function countWatchFeedItemsAfterPosition(
  items: AcpWatchFeedItem[],
  positionMs: number,
): number {
  return items.length - selectWatchFeedItemsForPosition(items, positionMs).length;
}

export function currentWatchFeedChapter(
  items: AcpWatchFeedItem[],
  positionMs: number,
): AcpWatchFeedItem["chapter"] {
  const safePositionMs = Number.isFinite(positionMs) ? Math.max(0, positionMs) : 0;
  const chapters = items
    .map((item) => item.chapter)
    .filter((chapter): chapter is NonNullable<typeof chapter> => chapter !== null)
    .filter((chapter) => chapter.startMs <= safePositionMs);

  return (
    chapters.find(
      (chapter) =>
        safePositionMs >= chapter.startMs && safePositionMs < chapter.endMs,
    ) ??
    chapters
      .slice()
      .sort((left, right) => right.startMs - left.startMs)[0] ??
    null
  );
}

/**
 * Reduce the durable append-only projection into the three cards that belong
 * to the current viewing position. The database keeps the full history, but
 * the watch-feed surface is a replace-in-place projection rather than a
 * second chat transcript.
 */
export function selectWatchFeedSlots(
  items: AcpWatchFeedItem[],
  positionMs: number,
): WatchFeedSlots {
  const visible = selectWatchFeedItemsForPosition(items, positionMs);
  const currentChapter = currentWatchFeedChapter(items, positionMs);
  const currentScoped = currentChapter
    ? visible.filter(
        (item) =>
          item.chapter?.id === currentChapter.id || item.chapter === null,
      )
    : visible;

  return {
    "watch-record":
      latestSlotItem(currentScoped, "watch-record") ??
      latestSlotItem(visible, "watch-record") ??
      null,
    recap:
      latestSlotItem(currentScoped, "recap") ??
      latestSlotItem(visible, "recap") ??
      null,
    highlights:
      latestSlotItem(currentScoped, "highlights") ??
      latestSlotItem(visible, "highlights") ??
      null,
  };
}

export function watchFeedSlotForItem(item: AcpWatchFeedItem): WatchFeedSlot {
  switch (item.itemType.trim().toLowerCase()) {
    case "recap":
    case "chapter_recap":
      return "recap";
    case "outlook":
    case "chapter_outlook":
    case "question":
    case "question_candidates":
    case "watch_point":
    case "watch_points":
    case "highlights":
      return "highlights";
    default:
      return "watch-record";
  }
}

function latestSlotItem(
  items: AcpWatchFeedItem[],
  slot: WatchFeedSlot,
): AcpWatchFeedItem | null {
  const candidates = items
    .filter((item) => watchFeedSlotForItem(item) === slot)
    .slice()
    .sort(compareWatchFeedItems);
  return candidates[candidates.length - 1] ?? null;
}

function compareWatchFeedItems(
  left: AcpWatchFeedItem,
  right: AcpWatchFeedItem,
): number {
  const leftChapterStart = left.chapter?.startMs ?? -1;
  const rightChapterStart = right.chapter?.startMs ?? -1;
  if (leftChapterStart !== rightChapterStart) {
    return leftChapterStart - rightChapterStart;
  }
  return (
    (left.publishedAtMs ?? Number.MIN_SAFE_INTEGER) -
      (right.publishedAtMs ?? Number.MIN_SAFE_INTEGER) ||
    left.id - right.id
  );
}
