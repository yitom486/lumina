import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { usePlayerStore } from "@/features/player";

import { useTypingPlaybackAnchor } from "./typingPlaybackAnchor";

describe("seedAnchorPositionMs", () => {
  it("seeds the anchor and survives the next draft change (P6-M3 shortcut)", () => {
    usePlayerStore.setState({ currentTimeMs: 600_000 });
    const { result } = renderHook(() => useTypingPlaybackAnchor());
    act(() => {
      result.current.seedAnchorPositionMs(192_000);
    });
    act(() => {
      result.current.handleDraftChange("解释这一段");
    });
    let anchor = 0;
    act(() => {
      anchor = result.current.consumeAnchorPositionMs();
    });
    // Seeded cue time wins over the live playback position.
    expect(anchor).toBe(192_000);
  });
});
