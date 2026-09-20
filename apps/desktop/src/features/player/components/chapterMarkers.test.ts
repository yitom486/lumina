import { describe, expect, it } from "vitest";

import { projectChapterMarkers } from "./chapterMarkers";

describe("projectChapterMarkers", () => {
  it("projects real chapters and ignores out-of-range entries", () => {
    expect(
      projectChapterMarkers(
        [
          { id: 2, startMs: 30_000, title: "中段" },
          { id: 1, startMs: 0, title: "开场" },
          { id: 9, startMs: 90_001, title: "越界" },
        ],
        90_000,
      ),
    ).toEqual([
      { id: 1, title: "开场", positionMs: 0, percent: 0 },
      { id: 2, title: "中段", positionMs: 30_000, percent: 33.33333333333333 },
    ]);
  });

  it("does not invent markers when chapters or duration are unavailable", () => {
    expect(projectChapterMarkers(undefined, 90_000)).toEqual([]);
    expect(projectChapterMarkers([], 90_000)).toEqual([]);
    expect(projectChapterMarkers([{ id: 1, startMs: 1_000 }], 0)).toEqual([]);
  });
});
