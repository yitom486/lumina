import { describe, expect, it, vi } from "vitest";

import { createLatestBoundsQueue } from "./surfaceBoundsQueue";

describe("createLatestBoundsQueue", () => {
  it("applies the newest rectangle after an in-flight stale write", async () => {
    let finishFirst: (() => void) | undefined;
    const firstWrite = new Promise<void>((resolve) => {
      finishFirst = resolve;
    });
    const write = vi
      .fn<(bounds: { width: number }) => Promise<void>>()
      .mockReturnValueOnce(firstWrite)
      .mockResolvedValue(undefined);
    const reportError = vi.fn();
    const queue = createLatestBoundsQueue(write, reportError);

    const first = queue({ x: 0, y: 0, width: 1_552, height: 900 });
    const latest = queue({ x: 0, y: 0, width: 2_000, height: 1_080 });
    finishFirst?.();
    await Promise.all([first, latest]);

    expect(write).toHaveBeenNthCalledWith(1, {
      x: 0,
      y: 0,
      width: 1_552,
      height: 900,
    });
    expect(write).toHaveBeenNthCalledWith(2, {
      x: 0,
      y: 0,
      width: 2_000,
      height: 1_080,
    });
    expect(reportError).not.toHaveBeenCalled();
  });
});
