import { describe, expect, it } from "vitest";

import {
  buildAnchoredVideoPromptContext,
  buildVideoPromptContext,
} from "./context";

describe("buildVideoPromptContext", () => {
  it("includes media identity, timing, and subtitle choice", () => {
    const ctx = buildVideoPromptContext({
      mediaPath: "D:\\videos\\demo.mp4",
      positionMs: 1500,
      durationMs: 60_000,
      subtitleChoiceId: "embedded:0",
    });

    expect(ctx?.mediaTitle).toBe("demo.mp4");
    expect(ctx?.subtitleChoiceId).toBe("embedded:0");
  });

  it("uses resolved online title instead of the URL tail", () => {
    const ctx = buildVideoPromptContext({
      mediaPath: "https://www.youtube.com/watch?v=demo",
      mediaTitle: "Resolved video title",
    });

    expect(ctx?.mediaTitle).toBe("Resolved video title");
  });
});

describe("buildAnchoredVideoPromptContext", () => {
  it("rebuilds the context at the typing anchor while preserving normalization", () => {
    const base = buildVideoPromptContext({
      mediaPath: "  D:\\videos\\demo.mp4  ",
      mediaTitle: "  Demo  ",
      positionMs: 9_000,
      durationMs: 60_000,
      subtitleChoiceId: "  embedded:0  ",
    });

    const anchored = buildAnchoredVideoPromptContext({
      base,
      anchorPositionMs: 1_500,
      durationMs: 60_000,
    });

    expect(anchored?.positionMs).toBe(1_500);
    expect(anchored?.mediaPath).toBe("D:\\videos\\demo.mp4");
    expect(anchored?.mediaTitle).toBe("Demo");
    expect(anchored?.durationMs).toBe(60_000);
    expect(anchored?.subtitleChoiceId).toBe("embedded:0");
  });
});
