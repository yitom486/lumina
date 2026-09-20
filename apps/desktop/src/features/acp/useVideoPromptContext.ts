import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { ytdlResolveKey } from "@lumina/query-keys";
import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { getCachedYtdlResolve } from "@/features/ytdl";
import { useAcpSettingsStore } from "@lumina/chat-ui/acpSettingsStore";
import { transcriptWindowRadiusSec } from "@lumina/chat-ui/types";

import { buildVideoPromptContext } from "./context";
import type { VideoPromptContext } from "./types";

/** Collect minimal playback context for the current prompt turn. */
export function useVideoPromptContext(): VideoPromptContext | undefined {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const positionMs = usePlayerStore((s) => s.currentTimeMs);
  const durationMs = usePlayerStore((s) => s.durationMs);
  const subtitleChoiceId = useTrackStore((s) => s.subtitleChoiceId);
  const transcriptWindowPreset = useAcpSettingsStore(
    (s) => s.transcriptWindowPreset,
  );

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
        transcriptWindowRadiusSec: transcriptWindowRadiusSec(
          transcriptWindowPreset,
        ),
      }),
    [
      path,
      onlineQuery.data?.title,
      onlineQuery.data?.durationMs,
      positionMs,
      durationMs,
      subtitleChoiceId,
      transcriptWindowPreset,
    ],
  );
}
