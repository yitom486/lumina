import { create } from "zustand";

import type { ChapterProgressEvent } from "./api";

type ChapterProgressState = {
  byTaskKey: Record<string, ChapterProgressEvent>;
  upsert: (event: ChapterProgressEvent) => void;
  clear: (taskKey: string) => void;
};

function shouldAccept(
  previous: ChapterProgressEvent | undefined,
  next: ChapterProgressEvent,
): boolean {
  if (!previous) return true;
  if (next.attemptCount !== previous.attemptCount) {
    return next.attemptCount > previous.attemptCount;
  }
  if (next.updatedAtMs > previous.updatedAtMs) return true;
  return (
    next.updatedAtMs === previous.updatedAtMs &&
    next.sequence >= previous.sequence
  );
}

export const useChapterProgressStore = create<ChapterProgressState>((set) => ({
  byTaskKey: {},
  upsert: (event) =>
    set((state) => {
      if (!shouldAccept(state.byTaskKey[event.taskKey], event)) {
        return state;
      }
      return {
        byTaskKey: { ...state.byTaskKey, [event.taskKey]: event },
      };
    }),
  clear: (taskKey) =>
    set((state) => {
      if (!(taskKey in state.byTaskKey)) return state;
      const next = { ...state.byTaskKey };
      delete next[taskKey];
      return { byTaskKey: next };
    }),
}));

export function chapterProgressForTask(
  taskKey: string | null | undefined,
): ChapterProgressEvent | null {
  if (!taskKey) return null;
  return useChapterProgressStore.getState().byTaskKey[taskKey] ?? null;
}
