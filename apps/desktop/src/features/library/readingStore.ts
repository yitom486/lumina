import { create } from "zustand";
import { persist } from "zustand/middleware";

import { useProgressStore } from "@/features/player/progressStore";

import type { EpisodeFile } from "./types";

export type ReadingStatus = "not-started" | "reading" | "done";

export const READING_STATUS_LABEL: Record<ReadingStatus, string> = {
  "not-started": "未开始",
  reading: "阅读中",
  done: "已完成",
};

type ReadingStore = {
  /** Manually completed media paths. Nothing else is persisted. */
  doneSet: Record<string, number>;
  markDone: (path: string) => void;
  unmarkDone: (path: string) => void;
};

export const useReadingStore = create<ReadingStore>()(
  persist(
    (set) => ({
      doneSet: {},
      markDone: (path) =>
        set((state) => ({
          doneSet: { ...state.doneSet, [path]: Date.now() },
        })),
      unmarkDone: (path) =>
        set((state) => {
          if (!(path in state.doneSet)) return state;
          const doneSet = { ...state.doneSet };
          delete doneSet[path];
          return { doneSet };
        }),
    }),
    {
      name: "lumina-reading-done",
      partialize: (state) => ({ doneSet: state.doneSet }),
    },
  ),
);

/**
 * Derived status: manual completion wins; playback progress implies reading;
 * Ended clearing progress never touches completion (separate store).
 */
export function episodeReadingStatus(
  path: string,
  hasProgress: boolean,
  doneSet: Record<string, number>,
): ReadingStatus {
  if (path in doneSet) return "done";
  if (hasProgress) return "reading";
  return "not-started";
}

/** First not-done episode in order; null when everything is done. */
export function findContinueTarget(
  episodes: EpisodeFile[],
  isDone: (path: string) => boolean,
): EpisodeFile | null {
  return episodes.find((episode) => !isDone(episode.path)) ?? null;
}

/** Episode right after the current file; null at the end or when absent. */
export function findNextEpisode(
  episodes: EpisodeFile[],
  currentPath: string | null,
): EpisodeFile | null {
  if (!currentPath) return episodes[0] ?? null;
  const index = episodes.findIndex((episode) => episode.path === currentPath);
  if (index < 0) return episodes[0] ?? null;
  return episodes[index + 1] ?? null;
}

export function hasPlaybackProgress(path: string): boolean {
  return useProgressStore.getState().getProgress(path) != null;
}
