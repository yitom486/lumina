import { useCallback, useRef } from "react";

import { usePlayerStore } from "@/features/player";

/** Max gap between draft edits before the next keystroke starts a new anchor. */
export const TYPING_ANCHOR_IDLE_MS = 10_000;

export type TypingAnchorState = {
  positionMs: number;
  lastInputAt: number;
};

export function nextTypingAnchorState(
  prev: TypingAnchorState | null,
  now: number,
  positionMs: number,
  hasContent: boolean,
  idleMs = TYPING_ANCHOR_IDLE_MS,
): TypingAnchorState | null {
  if (!hasContent) return null;
  if (!prev || now - prev.lastInputAt > idleMs) {
    return { positionMs, lastInputAt: now };
  }
  return { positionMs: prev.positionMs, lastInputAt: now };
}

export function useTypingPlaybackAnchor() {
  const anchorRef = useRef<TypingAnchorState | null>(null);

  const syncAnchorForDraft = useCallback((draft: string, now = Date.now()) => {
    const positionMs = usePlayerStore.getState().currentTimeMs;
    anchorRef.current = nextTypingAnchorState(
      anchorRef.current,
      now,
      positionMs,
      draft.trim().length > 0,
    );
  }, []);

  const clearTypingAnchor = useCallback(() => {
    anchorRef.current = null;
  }, []);

  /**
   * P6-M3 shortcut entry: seed the anchor at an explicit media time
   * (e.g. the cue the user asked about). A subsequent draft change within
   * the idle window keeps it — the freeze/consume machinery is untouched.
   */
  const seedAnchorPositionMs = useCallback((positionMs: number) => {
    anchorRef.current = { positionMs, lastInputAt: Date.now() };
  }, []);

  const consumeAnchorPositionMs = useCallback(() => {
    const fallback = usePlayerStore.getState().currentTimeMs;
    const positionMs = anchorRef.current?.positionMs ?? fallback;
    anchorRef.current = null;
    return positionMs;
  }, []);

  const handleDraftChange = useCallback(
    (next: string) => {
      syncAnchorForDraft(next);
    },
    [syncAnchorForDraft],
  );

  return {
    handleDraftChange,
    clearTypingAnchor,
    seedAnchorPositionMs,
    consumeAnchorPositionMs,
    syncAnchorForDraft,
  };
}
