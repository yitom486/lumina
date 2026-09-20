import { invoke } from "@tauri-apps/api/core";

import type { MediaMetadataContext } from "@/features/library";
import type { AgentProfilesHint } from "@lumina/chat-ui";
import type { MediaChapter } from "@lumina/contracts";

export type ChapterSpoilerBoundary =
  | "current_position"
  | "current_chapter"
  | "full_media";

export type ChapterEpisodeIdentity =
  | {
      kind: "authoritative";
      seriesStableId: string;
      episodeStableId: string;
      season: number;
      episode: number;
      seriesTitle?: string | null;
      title?: string | null;
    }
  | {
      kind: "legacy";
      reason?:
        | "metadata_unavailable"
        | "metadata_incomplete"
        | "context_lookup_failed"
        | "not_provided";
    };

export type ChapterSegmentationRequest = {
  mediaPath: string;
  episodeKey: string;
  episodeIdentity?: ChapterEpisodeIdentity;
  profileId?: string;
  profiles?: AgentProfilesHint;
  modelId?: string | null;
  reasoningEffort?: string | null;
  subtitleChoiceId?: string | null;
  positionMs?: number;
  spoilerBoundary?: ChapterSpoilerBoundary;
};

export type ChapterRetryAction = "retry" | "configure_agent";

export type ChapterDraftStatus =
  | "waiting_evidence"
  | "analyzing"
  | "generated"
  | "validation_failed";

export type ChapterDraftSnapshot = {
  id: number;
  stableId: string;
  startMs: number;
  endMs: number;
  title: string | null;
  mainline: string | null;
  status: ChapterDraftStatus;
  updatedAtMs: number;
};

export type ChapterSegmentationSnapshot = {
  id: number;
  taskKey: string;
  taskType: string;
  episodeId: number | null;
  chapterId: number | null;
  episodeIdentity: ChapterEpisodeIdentity;
  status: string;
  sessionId: string | null;
  promptVersion: string;
  outputContractVersion: string | null;
  attemptCount: number;
  retryCount: number;
  maxAttempts: number;
  failureCode: string | null;
  failureMessage: string | null;
  validationSummary: string | null;
  canRetry: boolean;
  retryAction: ChapterRetryAction | null;
  agentConfigured: boolean;
  outputJson: string | null;
  draftChapters: ChapterDraftSnapshot[];
  createdAtMs: number;
  updatedAtMs: number;
};

function isPositiveInteger(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0;
}

function nonEmpty(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

/**
 * Convert only an explicit library TMDb TV episode into a durable identity.
 * Missing or inconsistent metadata deliberately remains legacy; this helper
 * never infers a season, episode, or series from a path or filename.
 */
export function chapterEpisodeIdentityFromLibraryContext(
  context: MediaMetadataContext | null | undefined,
): Extract<ChapterEpisodeIdentity, { kind: "authoritative" }> | null {
  if (
    !context ||
    context.group.kind !== "series" ||
    context.item?.kind !== "episode" ||
    !isPositiveInteger(context.group.tmdbId) ||
    !isPositiveInteger(context.item.tmdbId) ||
    !isPositiveInteger(context.item.season) ||
    !isPositiveInteger(context.item.episode)
  ) {
    return null;
  }
  if (
    context.item.seriesTmdbId != null &&
    context.item.seriesTmdbId !== context.group.tmdbId
  ) {
    return null;
  }

  const season = context.item.season;
  const episode = context.item.episode;
  return {
    kind: "authoritative",
    seriesStableId: `tmdb:tv:${context.group.tmdbId}`,
    episodeStableId: `s${String(season).padStart(2, "0")}e${String(episode).padStart(2, "0")}`,
    season,
    episode,
    seriesTitle: nonEmpty(context.group.title),
    title: nonEmpty(context.item.title),
  };
}

export type ChapterCommandError = {
  code?: string;
  message?: string;
  details?: unknown;
};

/**
 * Read the durable chapter outline projection.  The outline is available
 * while the task is still running; it does not depend on assistant output.
 */
export function parseDraftChapters(
  snapshot: ChapterSegmentationSnapshot | undefined,
): ChapterDraftSnapshot[] {
  return snapshot?.draftChapters ?? [];
}

/**
 * Legacy compatibility parser.  The chapters panel no longer uses this
 * outputJson path; it remains only for old persisted tasks during migration.
 */
export function parseGeneratedChapters(
  snapshot: ChapterSegmentationSnapshot | undefined,
): MediaChapter[] {
  if (snapshot?.status !== "succeeded" || !snapshot.outputJson) return [];
  try {
    const value: unknown = JSON.parse(snapshot.outputJson);
    if (!isRecord(value) || !Array.isArray(value.chapters)) return [];
    return value.chapters.flatMap((chapter, index) => {
      if (!isRecord(chapter)) return [];
      const startMs = toFiniteNumber(chapter.start_ms);
      const endMs = toFiniteNumber(chapter.end_ms);
      if (startMs === null || endMs === null || endMs <= startMs) return [];
      const title = typeof chapter.title === "string" ? chapter.title.trim() : "";
      return [
        {
          id: index + 1,
          startMs,
          endMs,
          title: title || null,
        },
      ];
    });
  } catch {
    return [];
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function toFiniteNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function startChapterSegmentation(
  request: ChapterSegmentationRequest,
): Promise<ChapterSegmentationSnapshot> {
  return invoke<ChapterSegmentationSnapshot>("chapter_segmentation_start", {
    request,
  });
}

export function getChapterSegmentationStatus(
  request: ChapterSegmentationRequest,
): Promise<ChapterSegmentationSnapshot> {
  return invoke<ChapterSegmentationSnapshot>("chapter_segmentation_status", {
    request,
  });
}
