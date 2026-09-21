import { describe, expect, it } from "vitest";

import type { AcpWatchFeedItem } from "../api";
import {
  countWatchFeedItemsAfterPosition,
  currentWatchFeedChapter,
  selectWatchFeedItemsForPosition,
  selectWatchFeedSlots,
} from "./watchFeedProjection";

function item(
  id: number,
  startMs: number | null,
  endMs = startMs == null ? 0 : startMs + 10_000,
): AcpWatchFeedItem {
  return {
    id,
    episodeId: 1,
    chapterId: startMs == null ? null : id,
    revisionId: null,
    taskId: null,
    itemType: "chapter",
    source: "ai",
    content: `内容 ${id}`,
    spoilerLevel: "current_chapter",
    contentVersion: "v1",
    publishedAtMs: id,
    chapter:
      startMs == null
        ? null
        : {
            id,
            startMs,
            endMs,
            spoilerLevel: "current_chapter",
            title: `章节 ${id}`,
            mainline: `主线 ${id}`,
            status: "ready",
          },
    revision: null,
    questionCandidate: null,
    screenshotRefs: [],
    coverRef: null,
  };
}

describe("watch-feed position projection", () => {
  it("hides future chapter items while retaining session-level items", () => {
    const items = [item(1, 0), item(2, 30_000), item(3, null)];

    expect(selectWatchFeedItemsForPosition(items, 12_000).map((entry) => entry.id)).toEqual([
      1, 3,
    ]);
    expect(countWatchFeedItemsAfterPosition(items, 12_000)).toBe(1);
  });

  it("selects the chapter containing the current position", () => {
    const items = [item(1, 0), item(2, 30_000)];

    expect(currentWatchFeedChapter(items, 35_000)?.title).toBe("章节 2");
    expect(currentWatchFeedChapter(items, 15_000)?.title).toBe("章节 1");
  });

  it("keeps one latest item per semantic slot for the current chapter", () => {
    const currentRecord = item(1, 0);
    const sameChapter = currentRecord.chapter;
    const laterRecord = {
      ...item(2, 0),
      chapter: sameChapter,
      publishedAtMs: 2,
    };
    const recap = { ...item(3, 0), chapter: sameChapter, itemType: "chapter_recap" };
    const oldHighlight = { ...item(4, 0), chapter: sameChapter, itemType: "watch_point" };
    const currentHighlight = {
      ...item(5, 0),
      chapter: sameChapter,
      itemType: "question_candidates",
      publishedAtMs: 5,
    };
    const futureRecord = item(6, 30_000);

    const slots = selectWatchFeedSlots(
      [currentRecord, laterRecord, recap, oldHighlight, currentHighlight, futureRecord],
      10_000,
    );

    expect(slots["watch-record"]?.id).toBe(2);
    expect(slots.recap?.id).toBe(3);
    expect(slots.highlights?.id).toBe(5);
  });
});
