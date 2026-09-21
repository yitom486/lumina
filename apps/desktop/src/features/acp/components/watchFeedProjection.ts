import type { AcpWatchFeedItem } from "../api";

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
