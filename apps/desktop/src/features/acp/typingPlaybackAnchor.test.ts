import { describe, expect, it } from "vitest";

import {
  nextTypingAnchorState,
  TYPING_ANCHOR_IDLE_MS,
} from "./typingPlaybackAnchor";

describe("nextTypingAnchorState", () => {
  it("captures position on first non-empty draft", () => {
    expect(nextTypingAnchorState(null, 1_000, 42_000, true)).toEqual({
      positionMs: 42_000,
      lastInputAt: 1_000,
    });
  });

  it("clears anchor when draft becomes empty", () => {
    expect(
      nextTypingAnchorState(
        { positionMs: 42_000, lastInputAt: 1_000 },
        2_000,
        99_000,
        false,
      ),
    ).toBeNull();
  });

  it("keeps anchor within idle window", () => {
    const prev = { positionMs: 42_000, lastInputAt: 1_000 };
    expect(
      nextTypingAnchorState(
        prev,
        1_000 + TYPING_ANCHOR_IDLE_MS,
        88_000,
        true,
      ),
    ).toEqual({
      positionMs: 42_000,
      lastInputAt: 1_000 + TYPING_ANCHOR_IDLE_MS,
    });
  });

  it("re-anchors after idle window expires", () => {
    const prev = { positionMs: 42_000, lastInputAt: 1_000 };
    expect(
      nextTypingAnchorState(
        prev,
        1_000 + TYPING_ANCHOR_IDLE_MS + 1,
        88_000,
        true,
      ),
    ).toEqual({
      positionMs: 88_000,
      lastInputAt: 1_000 + TYPING_ANCHOR_IDLE_MS + 1,
    });
  });
});
