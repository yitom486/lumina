import { describe, expect, it } from "vitest";

import type { Cue } from "@/features/transcript/types";

import {
  cueIndicesInListRange,
  mergeCueIndices,
  toggleCueIndex,
} from "./noteQuoteSelection";

function cue(index: number, startMs: number, text: string): Cue {
  return { index, startMs, endMs: startMs + 1000, text };
}

describe("noteQuoteSelection", () => {
  const cues = [
    cue(1, 0, "a"),
    cue(2, 1000, "b"),
    cue(3, 2000, "c"),
    cue(4, 3000, "d"),
  ];

  it("cueIndicesInListRange is inclusive", () => {
    expect(cueIndicesInListRange(cues, 1, 3)).toEqual([2, 3, 4]);
    expect(cueIndicesInListRange(cues, 3, 0)).toEqual([1, 2, 3, 4]);
  });

  it("mergeCueIndices dedupes", () => {
    expect(mergeCueIndices([1, 2], [2, 3])).toEqual([1, 2, 3]);
  });

  it("toggleCueIndex adds and removes", () => {
    expect(toggleCueIndex([1], 2)).toEqual([1, 2]);
    expect(toggleCueIndex([1, 2], 1)).toEqual([2]);
  });
});
