import { formatTime } from "@/lib/format";
import { Slider } from "@/components/ui/slider";
import { useMediaInfoQuery } from "@/features/media";

import { usePlayerStore } from "../store";

export function SeekBar() {
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const durationMs = usePlayerStore((s) => s.durationMs);
  const setSeeking = usePlayerStore((s) => s.setSeeking);
  const setPreviewTime = usePlayerStore((s) => s.setPreviewTime);
  const seek = usePlayerStore((s) => s.seek);
  const mediaInfo = useMediaInfoQuery();

  // Prefer Rust/mpv duration; fall back to ffprobe so the bar isn't stuck at 0:00.
  const probeDuration = mediaInfo.data?.durationMs ?? 0;
  const effectiveDuration =
    durationMs > 0 ? durationMs : probeDuration > 0 ? probeDuration : 0;

  const canSeek =
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";
  const ready = canSeek && effectiveDuration > 0;
  const value = Math.min(currentTimeMs, effectiveDuration || 0);

  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <span className="w-11 shrink-0 tabular-nums text-xs text-muted-foreground">
        {formatTime(currentTimeMs)}
      </span>
      <Slider
        className="min-w-0 flex-1"
        min={0}
        max={Math.max(effectiveDuration, 1)}
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
        {formatTime(effectiveDuration)}
      </span>
    </div>
  );
}
