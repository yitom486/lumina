/** Chapter selection view-model (no Tauri, no stores, no invoke). */

import type { MediaChapter } from "@lumina/contracts";

export function findChapterAt(
  chapters: MediaChapter[],
  timeMs: number,
): MediaChapter | null {
  if (chapters.length === 0) return null;
  const hit = chapters.find((chapter) => {
    if (timeMs < chapter.startMs) return false;
    if (chapter.endMs == null) return true;
    return timeMs < chapter.endMs;
  });
  return hit ?? null;
}
