import { describe, expect, it } from "vitest";

import type { Cue } from "@/features/transcript/types";

import { buildSoftSegments } from "./softSegments";

function cue(index: number, startMs: number, endMs: number): Cue {
  return { index, startMs, endMs, text: `t${index}` };
}

describe("buildSoftSegments", () => {
  it("returns empty for no cues", () => {
    expect(buildSoftSegments([])).toEqual([]);
  });

  it("cuts before pauses and numbers mechanical titles", () => {
    const segments = buildSoftSegments([
      cue(0, 0, 900),
      cue(1, 1000, 1900),
      cue(2, 20_000, 20_900),
    ]);
    expect(segments).toHaveLength(2);
    expect(segments[0]).toMatchObject({ startMs: 0, endMs: 1900 });
    expect(segments[0]?.title).toBe("分段 1 · 0:00–0:01");
    expect(segments[1]).toMatchObject({ startMs: 20_000, endMs: 20_900 });
    expect(segments[1]?.title).toBe("分段 2 · 0:20–0:20");
  });

  it("cuts at the target length without a pause", () => {
    const cues = Array.from({ length: 10 }, (_, i) =>
      cue(i, i * 30_000, i * 30_000 + 900),
    );
    const segments = buildSoftSegments(cues, {
      pauseGapMs: 60_000,
      targetMs: 100_000,
      maxMs: 600_000,
    });
    // Cuts before cue 4 (120s) and cue 8 (240s).
    expect(segments.map((s) => s.startMs)).toEqual([0, 120_000, 240_000]);
  });

  it("maxMs guards runaway segments", () => {
    const cues = Array.from({ length: 5 }, (_, i) =>
      cue(i, i * 30_000, i * 30_000 + 900),
    );
    const segments = buildSoftSegments(cues, {
      pauseGapMs: 600_000,
      targetMs: 600_000,
      maxMs: 70_000,
    });
    expect(segments.map((s) => s.startMs)).toEqual([0, 90_000]);
  });

  it("sorts unsorted input and formats long titles", () => {
    const segments = buildSoftSegments([cue(1, 3_723_000, 3_724_000)]);
    expect(segments).toHaveLength(1);
    expect(segments[0]?.startMs).toBe(3_723_000);
    expect(segments[0]?.title).toContain("62:03");
  });
});
