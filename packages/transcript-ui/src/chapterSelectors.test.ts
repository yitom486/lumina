import { describe, expect, it } from "vitest";

import { findChapterAt } from "./chapterSelectors";

const chapters = [
  { id: 0, startMs: 0, endMs: 60_000, title: "开场" },
  { id: 1, startMs: 60_000, endMs: null, title: "正片" },
];

describe("findChapterAt", () => {
  it("returns null for an empty list", () => {
    expect(findChapterAt([], 500)).toBeNull();
  });

  it("hits chapters by start-inclusive, end-exclusive bounds", () => {
    expect(findChapterAt(chapters, 0)?.id).toBe(0);
    expect(findChapterAt(chapters, 59_999)?.id).toBe(0);
    expect(findChapterAt(chapters, 60_000)?.id).toBe(1);
  });

  it("treats missing endMs as open-ended", () => {
    expect(findChapterAt(chapters, 10_000_000)?.id).toBe(1);
  });

  it("returns null inside gaps and before the first chapter", () => {
    const gapped = [
      { id: 0, startMs: 10_000, endMs: 20_000 },
      { id: 1, startMs: 30_000, endMs: 40_000 },
    ];
    expect(findChapterAt(gapped, 5_000)).toBeNull();
    expect(findChapterAt(gapped, 25_000)).toBeNull();
    expect(findChapterAt(gapped, 35_000)?.id).toBe(1);
  });
});
