import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { ytdlResolveKey } from "@lumina/query-keys";
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

  const onlineQuery = useQuery({
    queryKey: ytdlResolveKey(path),
    queryFn: () => getCachedYtdlResolve(path as string),
    enabled: mediaReady && /^https?:\/\//i.test(path ?? ""),
    staleTime: Infinity,
  });

  return useMemo(
    () =>
      buildVideoPromptContext({
        mediaPath: path,
        mediaTitle: onlineQuery.data?.title,
        positionMs,
        durationMs: durationMs || onlineQuery.data?.durationMs || undefined,
        subtitleChoiceId,
      }),
    [
      path,
      onlineQuery.data?.title,
      onlineQuery.data?.durationMs,
      positionMs,
      durationMs,
      subtitleChoiceId,
    ],
  );
}
