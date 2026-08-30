import { formatTime } from "@/lib/format";

import { usePlayerStore } from "../store";

export function SeekBar() {
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const durationMs = usePlayerStore((s) => s.durationMs);
  const setSeeking = usePlayerStore((s) => s.setSeeking);
  const setPreviewTime = usePlayerStore((s) => s.setPreviewTime);
  const seek = usePlayerStore((s) => s.seek);

  const ready = status !== "Idle" && durationMs > 0;

  return (
    <div className="flex flex-wrap items-center gap-3">
      <span className="w-12 tabular-nums text-sm text-muted-foreground">
        {formatTime(currentTimeMs)}
      </span>
      <input
        type="range"
        className="min-w-[12rem] flex-1"
        min={0}
        max={Math.max(durationMs, 1)}
        step={100}
        value={Math.min(currentTimeMs, durationMs || 0)}
        disabled={!ready}
        onMouseDown={() => setSeeking(true)}
        onTouchStart={() => setSeeking(true)}
        onChange={(e) => setPreviewTime(Number(e.target.value))}
        onMouseUp={(e) => {
          void seek(Number(e.currentTarget.value));
        }}
        onTouchEnd={(e) => {
          void seek(Number(e.currentTarget.value));
        }}
        aria-label="Seek"
      />
      <span className="w-12 tabular-nums text-sm text-muted-foreground">
        {formatTime(durationMs)}
      </span>
    </div>
  );
}
