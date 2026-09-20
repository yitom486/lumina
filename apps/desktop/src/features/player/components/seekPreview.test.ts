import { describe, expect, it } from "vitest";

import {
  getSeekPreviewPercent,
  getSeekPreviewTimeMs,
  getSeekPreviewTooltipPercent,
} from "./seekPreview";

describe("seek preview helpers", () => {
  it("maps pointer positions to clamped media time", () => {
    const track = { left: 100, width: 400 };

    expect(getSeekPreviewTimeMs(100, track, 120_000)).toBe(0);
    expect(getSeekPreviewTimeMs(300, track, 120_000)).toBe(60_000);
    expect(getSeekPreviewTimeMs(700, track, 120_000)).toBe(120_000);
    expect(getSeekPreviewTimeMs(20, track, 120_000)).toBe(0);
  });

  it("returns no preview when the track or duration is unavailable", () => {
    expect(getSeekPreviewTimeMs(200, { left: 0, width: 0 }, 120_000)).toBeNull();
    expect(getSeekPreviewTimeMs(200, { left: 0, width: 400 }, 0)).toBeNull();
    expect(getSeekPreviewPercent(60_000, 120_000)).toBe(50);
    expect(getSeekPreviewPercent(180_000, 120_000)).toBe(100);
    expect(getSeekPreviewPercent(60_000, 0)).toBeNull();
  });

  it("keeps the preview bubble inside the seek track edges", () => {
    expect(getSeekPreviewTooltipPercent(0)).toBe(8);
    expect(getSeekPreviewTooltipPercent(50)).toBe(50);
    expect(getSeekPreviewTooltipPercent(100)).toBe(92);
    expect(getSeekPreviewTooltipPercent(0, 60)).toBe(49);
    expect(getSeekPreviewTooltipPercent(Number.NaN)).toBeNull();
  });
});
