import { type PointerEvent, useState } from "react";

import { useMediaInfoQuery } from "@/features/media";
import { formatTime } from "@/lib/format";

import { usePlayerStore } from "../store";
import { projectChapterMarkers } from "./chapterMarkers";
import { PlayerSlider } from "./PlayerSlider";
import { ResumeChip } from "./ResumeChip";
import {
  getSeekPreviewPercent,
  getSeekPreviewTimeMs,
  getSeekPreviewTooltipPercent,
} from "./seekPreview";

export function SeekBar() {
  const [previewTimeMs, setPreviewTimeMs] = useState<number | null>(null);
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const durationMs = usePlayerStore((s) => s.durationMs);
  const durationHintMs = usePlayerStore((s) => s.durationHintMs);
  const setSeeking = usePlayerStore((s) => s.setSeeking);
  const setPreviewTime = usePlayerStore((s) => s.setPreviewTime);
  const seek = usePlayerStore((s) => s.seek);
  const mediaInfo = useMediaInfoQuery();

  // Prefer mpv demux duration; then yt-dlp hint; then local ffprobe.
  const probeDuration = mediaInfo.data?.durationMs ?? 0;
  const effectiveDuration =
    durationMs > 0
      ? durationMs
      : (durationHintMs ?? 0) > 0
        ? (durationHintMs as number)
        : probeDuration > 0
          ? probeDuration
          : 0;

  const canSeek =
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";
  const ready = canSeek && effectiveDuration > 0;
  const value = Math.min(currentTimeMs, effectiveDuration || 0);
  const chapterMarkers = projectChapterMarkers(
    mediaInfo.data?.chapters,
    effectiveDuration,
  );
  const previewPercent =
    previewTimeMs === null
      ? null
      : getSeekPreviewPercent(previewTimeMs, effectiveDuration);
  const previewTooltipPercent =
    previewPercent === null
      ? null
      : getSeekPreviewTooltipPercent(previewPercent);

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!ready || event.pointerType === "touch") {
      setPreviewTimeMs(null);
      return;
    }

    const track = event.currentTarget.getBoundingClientRect();
    setPreviewTimeMs(
      getSeekPreviewTimeMs(
        event.clientX,
        { left: track.left, width: track.width },
        effectiveDuration,
      ),
    );
  };

  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <span className="w-11 shrink-0 tabular-nums text-xs text-muted-foreground">
        {formatTime(currentTimeMs)}
      </span>
      <div
        className="relative min-w-0 flex-1"
        onPointerMove={handlePointerMove}
        onPointerLeave={() => setPreviewTimeMs(null)}
      >
        <ResumeChip />
        {previewTooltipPercent !== null && previewTimeMs !== null ? (
          <span
            className="pointer-events-none absolute bottom-full z-20 mb-2 -translate-x-1/2 rounded-md border border-border/70 bg-popover px-2 py-1 text-[10px] tabular-nums text-popover-foreground shadow-md"
            role="tooltip"
            style={{ left: `${previewTooltipPercent}%` }}
          >
            {formatTime(previewTimeMs)}
          </span>
        ) : null}
        <div
          className="pointer-events-none absolute inset-x-0 top-1/2 z-10 h-3 -translate-y-1/2"
          aria-hidden="true"
        >
          {chapterMarkers.map((marker) => (
            <span
              key={`${marker.id}-${marker.positionMs}`}
              className="absolute top-1/2 h-3 w-px -translate-x-1/2 -translate-y-1/2 bg-playback-accent/80"
              style={{ left: `${marker.percent}%` }}
              title={marker.title ?? undefined}
            />
          ))}
        </div>
        <PlayerSlider
          className="min-w-0 w-full [&_[data-radix-slider-range]]:bg-playback-accent [&_[data-radix-slider-thumb]]:border-playback-accent/50 [&_[data-radix-slider-thumb]]:bg-playback-accent"
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
          aria-valuetext={`${formatTime(value)} / ${formatTime(effectiveDuration)}`}
        />
      </div>
      <span className="w-11 shrink-0 tabular-nums text-xs text-muted-foreground">
        {formatTime(effectiveDuration)}
      </span>
    </div>
  );
}
