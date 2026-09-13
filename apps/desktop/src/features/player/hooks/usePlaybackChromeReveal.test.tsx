import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlaybackChromeReveal } from "./usePlaybackChromeReveal";

beforeEach(() => {
  Object.defineProperty(window, "innerHeight", {
    configurable: true,
    value: 800,
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("usePlaybackChromeReveal", () => {
  it("stays visible when cinema mode is off", () => {
    const { result } = renderHook(() => usePlaybackChromeReveal(false));
    expect(result.current.visible).toBe(true);
  });

  it("reveals when the cursor nears the bottom in fullscreen mode", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => usePlaybackChromeReveal(true));
    expect(result.current.visible).toBe(false);

    act(() => {
      window.dispatchEvent(
        new MouseEvent("mousemove", { clientY: 799, clientX: 100 }),
      );
    });
    expect(result.current.visible).toBe(true);

    act(() => {
      vi.advanceTimersByTime(2800);
    });
    expect(result.current.visible).toBe(false);
  });
});
