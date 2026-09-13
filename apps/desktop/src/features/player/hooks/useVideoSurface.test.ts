import { describe, expect, it } from "vitest";

import { nativeSurfaceMode } from "./useVideoSurface";

describe("nativeSurfaceMode", () => {
  it("preserves the HWND until the Rust runtime snapshot is known", () => {
    expect(nativeSurfaceMode(false, "Idle", null)).toBe("preserve");
    expect(nativeSurfaceMode(false, "Playing", "movie.mp4")).toBe("preserve");
  });

  it("shows the HWND only for a loaded playable state", () => {
    expect(nativeSurfaceMode(true, "Playing", "movie.mp4")).toBe("show");
    expect(nativeSurfaceMode(true, "Paused", "movie.mp4")).toBe("show");
    expect(nativeSurfaceMode(true, "Ended", "movie.mp4")).toBe("show");
  });

  it("hides the HWND for a synchronized empty or error state", () => {
    expect(nativeSurfaceMode(true, "Idle", null)).toBe("hide");
    expect(nativeSurfaceMode(true, "Error", "broken.mp4")).toBe("hide");
  });
});
