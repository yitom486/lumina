/** Persisted per-file playback position (localStorage). Not the live player mirror. */

import { create } from "zustand";
import { persist } from "zustand/middleware";

export type ProgressEntry = {
  positionMs: number;
  updatedAt: number;
};

type ProgressState = {
  byPath: Record<string, ProgressEntry>;
  saveProgress: (path: string, positionMs: number) => void;
  clearProgress: (path: string) => void;
  getProgress: (path: string) => ProgressEntry | null;
};

const MIN_SAVE_MS = 5_000;

export const useProgressStore = create<ProgressState>()(
  persist(
    (set, get) => ({
      byPath: {},

      saveProgress: (path, positionMs) => {
        if (!path || positionMs < MIN_SAVE_MS) return;
        set((state) => ({
          byPath: {
            ...state.byPath,
            [path]: { positionMs, updatedAt: Date.now() },
          },
        }));
      },

      clearProgress: (path) => {
        set((state) => {
          const next = { ...state.byPath };
          delete next[path];
          return { byPath: next };
        });
      },

      getProgress: (path) => get().byPath[path] ?? null,
    }),
    {
      name: "lumina-playback-progress",
      partialize: (state) => ({ byPath: state.byPath }),
    },
  ),
);

/** Whether a saved position should trigger the resume dialog. */
export function shouldOfferResume(
  positionMs: number,
  durationMs: number,
): boolean {
  if (positionMs < MIN_SAVE_MS) return false;
  if (durationMs > 0 && positionMs >= durationMs - MIN_SAVE_MS) return false;
  return true;
}
