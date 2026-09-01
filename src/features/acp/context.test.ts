import { describe, expect, it } from "vitest";

import {
  activeChapterTitle,
  buildVideoPromptContext,
  notesExcerptNear,
  transcriptExcerptAround,
} from "./context";

describe("buildVideoPromptContext", () => {
  it("includes chapter, subtitle choice, and notes excerpts", () => {
    const ctx = buildVideoPromptContext({
      mediaPath: "D:\\videos\\demo.mp4",
      positionMs: 1500,
      durationMs: 60_000,
      chapters: [{ id: 1, startMs: 0, endMs: 5000, title: "Intro" }],
      subtitleChoiceId: "embedded:0",
      notes: [
        {
          id: "n1",
          mediaPath: "D:\\videos\\demo.mp4",
          positionMs: 1400,
          body: "重点",
          createdAt: "",
          updatedAt: "",
        },
      ],
    });

    expect(ctx?.mediaTitle).toBe("demo.mp4");
    expect(ctx?.chapterTitle).toBe("Intro");
    expect(ctx?.subtitleChoiceId).toBe("embedded:0");
    expect(ctx?.notesExcerpt).toContain("重点");
  });
});

describe("activeChapterTitle", () => {
  it("returns active chapter title", () => {
    expect(
      activeChapterTitle(
        [
          { id: 1, startMs: 0, endMs: 1000, title: "A" },
          { id: 2, startMs: 1000, endMs: 2000, title: "B" },
        ],
        1200,
      ),
    ).toBe("B");
  });
});

describe("transcriptExcerptAround", () => {
  it("marks active cue", () => {
    const excerpt = transcriptExcerptAround(
      [{ index: 0, startMs: 0, endMs: 1000, text: "hello" }],
      500,
    );
    expect(excerpt).toContain("▶");
    expect(excerpt).toContain("hello");
  });
});

describe("notesExcerptNear", () => {
  it("filters by radius", () => {
    const excerpt = notesExcerptNear(
      [
        {
          id: "1",
          mediaPath: "x",
          positionMs: 1000,
          body: "near",
          createdAt: "",
          updatedAt: "",
        },
        {
          id: "2",
          mediaPath: "x",
          positionMs: 999_000,
          body: "far",
          createdAt: "",
          updatedAt: "",
        },
      ],
      1000,
      5000,
    );
    expect(excerpt).toContain("near");
    expect(excerpt).not.toContain("far");
  });
});
