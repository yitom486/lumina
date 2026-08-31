import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { useMediaInfoQuery } from "@/features/media";
import { listNotes } from "@/features/notes/api";
import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { loadSubtitleChoice, listSubtitleChoices } from "@/features/transcript/api";

import { buildVideoPromptContext } from "./context";
import type { VideoPromptContext } from "./types";

/** Collect playback / transcript / notes context for the current prompt turn. */
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

  const choicesQuery = useQuery({
    queryKey: ["subtitleChoices", path],
    queryFn: () => listSubtitleChoices(path as string),
    enabled: mediaReady,
    staleTime: 60_000,
  });

  const selectedChoice = choicesQuery.data?.find((c) => c.id === subtitleChoiceId);
  const canLoadTranscript = Boolean(
    subtitleChoiceId && selectedChoice?.supported,
  );

  const transcriptQuery = useQuery({
    queryKey: ["transcript", path, subtitleChoiceId],
    queryFn: () => loadSubtitleChoice(path as string, subtitleChoiceId as string),
    enabled: mediaReady && canLoadTranscript,
    staleTime: 60_000,
  });

  return useMemo(
    () =>
      buildVideoPromptContext({
        mediaPath: path,
        positionMs,
        durationMs,
        chapters: mediaQuery.data?.chapters,
        transcriptCues: transcriptQuery.data?.cues,
        notes: notesQuery.data,
      }),
    [
      path,
      positionMs,
      durationMs,
      mediaQuery.data?.chapters,
      transcriptQuery.data?.cues,
      notesQuery.data,
    ],
  );
}
