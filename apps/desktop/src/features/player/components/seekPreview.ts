export type SeekTrackRect = {
  left: number;
  width: number;
};

/** Convert a pointer position on the seek track into a clamped media time. */
export function getSeekPreviewTimeMs(
  clientX: number,
  track: SeekTrackRect,
  durationMs: number,
): number | null {
  if (
    !Number.isFinite(clientX) ||
    !Number.isFinite(track.left) ||
    !Number.isFinite(track.width) ||
    track.width <= 0 ||
    !Number.isFinite(durationMs) ||
    durationMs <= 0
  ) {
    return null;
  }

  const progress = Math.min(1, Math.max(0, (clientX - track.left) / track.width));
  return Math.round(progress * durationMs);
}

export function getSeekPreviewPercent(
  previewTimeMs: number,
  durationMs: number,
): number | null {
  if (
    !Number.isFinite(previewTimeMs) ||
    !Number.isFinite(durationMs) ||
    durationMs <= 0
  ) {
    return null;
  }

  return Math.min(100, Math.max(0, (previewTimeMs / durationMs) * 100));
}

/** Keep the hover bubble inside the seek track at both media edges. */
export function getSeekPreviewTooltipPercent(
  percent: number,
  edgePaddingPercent = 8,
): number | null {
  if (!Number.isFinite(percent) || !Number.isFinite(edgePaddingPercent)) {
    return null;
  }

  const padding = Math.min(49, Math.max(0, edgePaddingPercent));
  return Math.min(100 - padding, Math.max(padding, percent));
}
