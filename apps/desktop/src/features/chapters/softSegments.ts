import { formatTime } from "@/lib/format";
import type { Cue } from "@/features/transcript/types";

export type SoftSegment = {
  id: string;
  startMs: number;
  endMs: number;
  /** Mechanical label only (`分段 N · mm:ss–mm:ss`); never a generated topic. */
  title: string;
};

export type SoftSegmentOptions = {
  /** Cut before a cue following a pause this long (or longer). */
  pauseGapMs?: number;
  /** Cut before the first cue starting this far into the segment. */
  targetMs?: number;
  /** Hard guard: cut regardless once reached. */
  maxMs?: number;
};

const DEFAULT_PAUSE_GAP_MS = 8_000;
const DEFAULT_TARGET_MS = 180_000;
const DEFAULT_MAX_MS = 600_000;

/**
 * P6-M5 mechanical segmentation over subtitle cues. Chapters (container
 * metadata) always win; this only fills the gap, and the UI must label it
 * as mechanical — never as semantic chapters.
 */
export function buildSoftSegments(
  cues: Cue[],
  options?: SoftSegmentOptions,
): SoftSegment[] {
  const pauseGapMs = options?.pauseGapMs ?? DEFAULT_PAUSE_GAP_MS;
  const targetMs = options?.targetMs ?? DEFAULT_TARGET_MS;
  const maxMs = options?.maxMs ?? DEFAULT_MAX_MS;
  const ordered = [...cues].sort((a, b) => a.startMs - b.startMs);
  if (ordered.length === 0) return [];

  const groups: Cue[][] = [];
  let current: Cue[] = [];
  let segStart = 0;
  let segEnd = 0;
  for (const cue of ordered) {
    if (current.length === 0) {
      current = [cue];
      segStart = cue.startMs;
      segEnd = cue.endMs;
      continue;
    }
    const gap = cue.startMs - segEnd;
    const span = cue.startMs - segStart;
    if (gap >= pauseGapMs || span >= targetMs || span >= maxMs) {
      groups.push(current);
      current = [cue];
      segStart = cue.startMs;
      segEnd = cue.endMs;
    } else {
      current.push(cue);
      segEnd = Math.max(segEnd, cue.endMs);
    }
  }
  if (current.length > 0) groups.push(current);

  return groups.map((group, index) => {
    const startMs = group[0]?.startMs ?? 0;
    const endMs = group[group.length - 1]?.endMs ?? startMs;
    return {
      id: `soft-${index}`,
      startMs,
      endMs,
      title: `分段 ${index + 1} · ${formatTime(startMs)}–${formatTime(endMs)}`,
    };
  });
}
