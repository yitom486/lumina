import type { MediaChapter } from "@lumina/contracts";

export type ChapterMarker = {
  id: number;
  title: string | null;
  positionMs: number;
  percent: number;
};

/** Project only real container/online chapter data onto the seek track. */
export function projectChapterMarkers(
  chapters: MediaChapter[] | null | undefined,
  durationMs: number,
): ChapterMarker[] {
  if (!chapters?.length || !Number.isFinite(durationMs) || durationMs <= 0) {
    return [];
  }

  const seen = new Set<string>();
  return chapters
    .filter(
      (chapter) =>
        Number.isFinite(chapter.startMs) &&
        chapter.startMs >= 0 &&
        chapter.startMs <= durationMs,
    )
    .sort((left, right) => left.startMs - right.startMs)
    .filter((chapter) => {
      const key = `${chapter.id}:${chapter.startMs}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .map((chapter) => ({
      id: chapter.id,
      title: chapter.title?.trim() || null,
      positionMs: chapter.startMs,
      percent: (chapter.startMs / durationMs) * 100,
    }));
}
