import { describe, expect, it } from "vitest";

import { activeCueIndex, LANG_PRESETS } from "./cueSelectors";

const cues = [
  { index: 0, startMs: 0, endMs: 1000, text: "a" },
  { index: 1, startMs: 1000, endMs: 2000, text: "b" },
];

describe("activeCueIndex", () => {
  it("returns -1 before the first cue and for an empty list", () => {
    expect(activeCueIndex([], 500)).toBe(-1);
    expect(activeCueIndex(cues, -1)).toBe(-1);
  });

  it("matches start-inclusive, end-exclusive boundaries", () => {
    expect(activeCueIndex(cues, 0)).toBe(0);
    expect(activeCueIndex(cues, 999)).toBe(0);
    expect(activeCueIndex(cues, 1000)).toBe(1);
    expect(activeCueIndex(cues, 2000)).toBe(-1);
  });
});

describe("LANG_PRESETS", () => {
  it("offers the four translation targets", () => {
    expect(LANG_PRESETS.map((preset) => preset.value)).toEqual([
      "en",
      "zh",
      "ja",
      "ko",
    ]);
  });
});
