import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { useMediaInfoQuery } from "@/features/media";
import { listNotes } from "@/features/notes/api";
import { notesKey } from "@/features/notes/queries";
import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { getCachedYtdlResolve } from "@/features/ytdl";

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
  const onlineQuery = useQuery({
    queryKey: ["ytdl-resolve", path],
    queryFn: () => getCachedYtdlResolve(path as string),
    enabled: mediaReady && /^https?:\/\//i.test(path ?? ""),
    staleTime: Infinity,
  });

  const notesQuery = useQuery({
    queryKey: notesKey(path),
    queryFn: () => listNotes(path as string),
    enabled: mediaReady,
    staleTime: 15_000,
  });

  return useMemo(
    () =>
      buildVideoPromptContext({
        mediaPath: path,
        mediaTitle: onlineQuery.data?.title,
        positionMs,
        durationMs: durationMs || onlineQuery.data?.durationMs || undefined,
        chapters: mediaQuery.data?.chapters ?? onlineQuery.data?.chapters,
        subtitleChoiceId,
        notes: notesQuery.data,
      }),
    [
      path,
      onlineQuery.data?.title,
      onlineQuery.data?.durationMs,
      onlineQuery.data?.chapters,
      positionMs,
      durationMs,
      mediaQuery.data?.chapters,
      subtitleChoiceId,
      notesQuery.data,
    ],
  );
}
