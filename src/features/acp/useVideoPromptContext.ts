import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { useMediaInfoQuery } from "@/features/media";
import { listNotes } from "@/features/notes/api";
import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";

import { buildVideoPromptContext } from "./context";
import type { VideoPromptContext } from "./types";

/** Collect minimal playback context for the current prompt turn. */
export function useVideoPromptContext(): VideoPromptContext | undefined {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const positionMs = usePlayerStore((s) => s.currentTimeMs);
  const durationMs = usePlayerStore((s) => s.durationMs);
  const subtitleChoiceId = useTrackStore((s) => s.subtitleChoiceId);

  const mediaReady =
    Boolean(path) &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  const mediaQuery = useMediaInfoQuery();

  const notesQuery = useQuery({
    queryKey: ["notes", path],
    queryFn: () => listNotes(path as string),
    enabled: mediaReady,
    staleTime: 15_000,
  });

  return useMemo(
    () =>
      buildVideoPromptContext({
        mediaPath: path,
        positionMs,
        durationMs,
        chapters: mediaQuery.data?.chapters,
        subtitleChoiceId,
        notes: notesQuery.data,
      }),
    [
      path,
      positionMs,
      durationMs,
      mediaQuery.data?.chapters,
      subtitleChoiceId,
      notesQuery.data,
    ],
  );
}
