import { formatTime } from "@/lib/format";
import { Slider } from "@/components/ui/slider";

import { usePlayerStore } from "../store";

export function SeekBar() {
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const durationMs = usePlayerStore((s) => s.durationMs);
  const setSeeking = usePlayerStore((s) => s.setSeeking);
  const setPreviewTime = usePlayerStore((s) => s.setPreviewTime);
  const seek = usePlayerStore((s) => s.seek);

  const ready = status !== "Idle" && durationMs > 0;
  const value = Math.min(currentTimeMs, durationMs || 0);

  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <span className="w-11 shrink-0 tabular-nums text-xs text-muted-foreground">
        {formatTime(currentTimeMs)}
      </span>
      <Slider
        className="min-w-0 flex-1"
        min={0}
        max={Math.max(durationMs, 1)}
        step={100}
        value={[value]}
        disabled={!ready}
        onValueChange={(vals) => {
          setSeeking(true);
          setPreviewTime(vals[0] ?? 0);
        }}
        onValueCommit={(vals) => {
          void seek(vals[0] ?? 0);
        }}
        aria-label="进度"
      />
      <span className="w-11 shrink-0 tabular-nums text-xs text-muted-foreground">
        {formatTime(durationMs)}
      </span>
    </div>
  );
}
