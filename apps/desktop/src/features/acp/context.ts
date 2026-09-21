import type { VideoPromptContext } from "./types";

export function fileNameFromPath(path: string): string {
  const normalized = path.replace(/[/\\]+$/, "");
  const idx = Math.max(
    normalized.lastIndexOf("/"),
    normalized.lastIndexOf("\\"),
  );
  return idx >= 0 ? normalized.slice(idx + 1) : normalized;
}

export function buildVideoPromptContext(input: {
  mediaPath?: string | null;
  mediaTitle?: string | null;
  positionMs?: number;
  durationMs?: number;
  subtitleChoiceId?: string | null;
  transcriptWindowRadiusSec?: number | null;
}): VideoPromptContext | undefined {
  const mediaPath = input.mediaPath?.trim();
  if (!mediaPath) return undefined;

  const context: VideoPromptContext = {
    mediaPath,
    mediaTitle: input.mediaTitle?.trim() || fileNameFromPath(mediaPath),
    positionMs: input.positionMs,
    durationMs: input.durationMs,
    subtitleChoiceId: input.subtitleChoiceId?.trim() || undefined,
    transcriptWindowRadiusSec: input.transcriptWindowRadiusSec ?? undefined,
  };

  return context;
}

/** Rebuild prompt context so every time-derived field uses the typing anchor. */
export function buildAnchoredVideoPromptContext(input: {
  base?: VideoPromptContext;
  anchorPositionMs: number;
  durationMs?: number;
}): VideoPromptContext | undefined {
  const mediaPath = input.base?.mediaPath?.trim();
  if (!mediaPath) return undefined;

  return buildVideoPromptContext({
    mediaPath,
    mediaTitle: input.base?.mediaTitle,
    positionMs: input.anchorPositionMs,
    durationMs: input.durationMs ?? input.base?.durationMs ?? undefined,
    subtitleChoiceId: input.base?.subtitleChoiceId,
    transcriptWindowRadiusSec: input.base?.transcriptWindowRadiusSec,
  });
}
