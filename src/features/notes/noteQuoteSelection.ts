import type { Cue } from "@/features/transcript/types";

/** List-index range → stable cue.index values (inclusive). */
export function cueIndicesInListRange(
  cues: Cue[],
  fromListIndex: number,
  toListIndex: number,
): number[] {
  if (cues.length === 0) return [];
  const lo = Math.max(0, Math.min(fromListIndex, toListIndex));
  const hi = Math.min(cues.length - 1, Math.max(fromListIndex, toListIndex));
  return cues.slice(lo, hi + 1).map((cue) => cue.index);
}

export function mergeCueIndices(
  current: number[],
  added: number[],
): number[] {
  return [...new Set([...current, ...added])];
}

export function toggleCueIndex(current: number[], cueIndex: number): number[] {
  if (current.includes(cueIndex)) {
    return current.filter((index) => index !== cueIndex);
  }
  return [...current, cueIndex];
}

export function indicesAroundPlayback(
  cues: Cue[],
  positionMs: number,
  context: number,
): number[] {
  if (cues.length === 0) return [];
  const center = activeCueListIndex(cues, positionMs);
  const start = Math.max(0, center - context);
  const end = Math.min(cues.length - 1, center + context);
  return cues.slice(start, end + 1).map((cue) => cue.index);
}

export function activeCueListIndex(cues: Cue[], positionMs: number): number {
  const hit = cues.findIndex(
    (cue) => positionMs >= cue.startMs && positionMs < cue.endMs,
  );
  if (hit >= 0) return hit;
  for (let i = cues.length - 1; i >= 0; i -= 1) {
    if (cues[i].startMs <= positionMs) return i;
  }
  return 0;
}
