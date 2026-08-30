/** Transparent placeholder; actual pixels come from native libmpv HWND. */

import { useVideoSurface } from "../hooks/useVideoSurface";

export function VideoSurface() {
  const { surfaceRef } = useVideoSurface();

  return (
    <div
      ref={surfaceRef}
      className="relative min-h-0 flex-1 bg-black"
      aria-label="Native video surface"
    />
  );
}
