import type { MediaChapter } from "@/features/media/types";
import type { Note } from "@/features/notes/types";
import type { Cue } from "@/features/transcript/types";
import { formatTime } from "@/lib/format";

import type { VideoPromptContext } from "./types";

export function fileNameFromPath(path: string): string {
  const normalized = path.replace(/[/\\]+$/, "");
  const idx = Math.max(
    normalized.lastIndexOf("/"),
    normalized.lastIndexOf("\\"),
  );
  return idx >= 0 ? normalized.slice(idx + 1) : normalized;
}

export function activeChapterTitle(
  chapters: MediaChapter[] | undefined,
  positionMs: number,
): string | undefined {
  if (!chapters?.length) return undefined;
  const chapter = chapters.find(
    (c) =>
      positionMs >= c.startMs &&
      (c.endMs == null || positionMs < c.endMs),
  );
  return chapter?.title?.trim() || undefined;
}

export function transcriptExcerptAround(
  cues: Cue[] | undefined,
  positionMs: number,
  window = 2,
): string | undefined {
  if (!cues?.length) return undefined;
  const active = cues.findIndex(
    (c) => positionMs >= c.startMs && positionMs < c.endMs,
  );
  const center = active >= 0 ? active : cues.findIndex((c) => c.startMs > positionMs);
  if (center < 0) return undefined;
  const start = Math.max(0, center - window);
  const end = Math.min(cues.length - 1, center + window);
  const lines = cues.slice(start, end + 1).map((cue, offset) => {
    const cueIndex = start + offset;
    const marker = cueIndex === active ? "▶ " : "  ";
    return `${marker}[${formatTime(cue.startMs)}] ${cue.text.trim()}`;
  });
  return lines.join("\n");
}

export function notesExcerptNear(
  notes: Note[] | undefined,
  positionMs: number,
  radiusMs = 120_000,
  limit = 5,
): string | undefined {
  if (!notes?.length) return undefined;
  const nearby = notes
    .filter((note) => Math.abs(note.positionMs - positionMs) <= radiusMs)
    .sort(
      (a, b) =>
        Math.abs(a.positionMs - positionMs) -
        Math.abs(b.positionMs - positionMs),
    )
    .slice(0, limit);
  if (nearby.length === 0) return undefined;
  return nearby
    .map((note) => `- [${formatTime(note.positionMs)}] ${note.body.trim()}`)
    .join("\n");
}

export function buildVideoPromptContext(input: {
  mediaPath?: string | null;
  positionMs?: number;
  durationMs?: number;
  chapters?: MediaChapter[];
  transcriptCues?: Cue[];
  notes?: Note[];
}): VideoPromptContext | undefined {
  const mediaPath = input.mediaPath?.trim();
  if (!mediaPath) return undefined;

  const context: VideoPromptContext = {
    mediaPath,
    mediaTitle: fileNameFromPath(mediaPath),
    positionMs: input.positionMs,
    durationMs: input.durationMs,
    chapterTitle: activeChapterTitle(input.chapters, input.positionMs ?? 0),
    transcriptExcerpt: transcriptExcerptAround(
      input.transcriptCues,
      input.positionMs ?? 0,
    ),
    notesExcerpt: notesExcerptNear(input.notes, input.positionMs ?? 0),
  };

  return context;
}
