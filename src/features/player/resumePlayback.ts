import { shouldOfferResume } from "./progressStore";

/** How close the player must be to the saved spot before we show the resume chip. */
export const RESUME_MATCH_TOLERANCE_MS = 2_500;

/** mpv may reject the first seek right after loadfile — retry with backoff. */
export const RESUME_SEEK_DELAYS_MS = [0, 120, 250, 500, 900] as const;

export type ResumeAttemptResult =
  | { kind: "none" }
  | { kind: "toast"; positionMs: number };

export function planResumeToast(
  savedPositionMs: number | null | undefined,
  durationMs: number,
  actualPositionMs: number,
): ResumeAttemptResult {
  if (savedPositionMs == null) return { kind: "none" };
  if (!shouldOfferResume(savedPositionMs, durationMs)) return { kind: "none" };
  if (Math.abs(actualPositionMs - savedPositionMs) > RESUME_MATCH_TOLERANCE_MS) {
    return { kind: "none" };
  }
  return { kind: "toast", positionMs: actualPositionMs };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

/** Seek to saved progress; returns landed position or null when all attempts fail. */
export async function seekForResume(
  targetMs: number,
  seek: (positionMs: number) => Promise<{ currentTimeMs: number }>,
  delaysMs: readonly number[] = RESUME_SEEK_DELAYS_MS,
): Promise<number | null> {
  for (const delayMs of delaysMs) {
    if (delayMs > 0) {
      await sleep(delayMs);
    }
    try {
      const snapshot = await seek(targetMs);
      if (
        Math.abs(snapshot.currentTimeMs - targetMs) <=
        RESUME_MATCH_TOLERANCE_MS
      ) {
        return snapshot.currentTimeMs;
      }
    } catch {
      // demux not ready yet — try again
    }
  }
  return null;
}
