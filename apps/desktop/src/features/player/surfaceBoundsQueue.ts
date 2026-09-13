export type SurfaceBounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

/**
 * Native HWND updates are asynchronous. Resize, fullscreen and React reflow
 * can measure several rectangles at once; serialize writes so an old rectangle
 * can never land after the latest one.
 */
export function createLatestBoundsQueue(
  write: (bounds: SurfaceBounds) => Promise<void>,
  onError: (error: unknown) => void,
): (bounds: SurfaceBounds) => Promise<void> {
  let pending: SurfaceBounds | null = null;
  let flushing: Promise<void> | null = null;

  const flush = async () => {
    while (pending) {
      const next = pending;
      pending = null;
      try {
        await write(next);
      } catch (error) {
        onError(error);
      }
    }
  };

  return (bounds) => {
    pending = bounds;
    if (!flushing) {
      flushing = flush().finally(() => {
        flushing = null;
      });
    }
    return flushing;
  };
}
