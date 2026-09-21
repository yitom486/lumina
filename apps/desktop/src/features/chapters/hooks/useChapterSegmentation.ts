import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage } from "@/lib/format";
import { getMediaMetadataContext } from "@/features/library";
import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";

import {
  profilesHintFromStore,
  useAcpProfilesStore,
  useAcpSettingsStore,
} from "@lumina/chat-ui";

import {
  getChapterSegmentationStatus,
  chapterEpisodeIdentityFromLibraryContext,
  startChapterSegmentation,
  type ChapterCommandError,
  type ChapterSegmentationRequest,
  type ChapterSegmentationSnapshot,
} from "../api";
import { chapterSegmentationKey } from "../queries";
import { useChapterProgressStore } from "../progressStore";

export type ChapterSegmentationUiStatus =
  | "unavailable"
  | "ready"
  | "pending"
  | "validation_failure"
  | "completed"
  | "failed";

export type ChapterIdentityResolution =
  | "unavailable"
  | "pending"
  | "authoritative"
  | "legacy";

function isMissingTaskError(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    (error as ChapterCommandError).code === "NotFound"
  );
}

function isPendingTask(status: string | undefined): boolean {
  return status === "pending" || status === "queued" || status === "running";
}

export function useChapterSegmentation() {
  const mediaPath = usePlayerStore((state) => state.currentFile);
  const positionMs = usePlayerStore((state) => state.currentTimeMs);
  const subtitleChoiceId = useTrackStore((state) => state.subtitleChoiceId);
  const activeProfileId = useAcpProfilesStore((state) => state.activeProfileId);
  const profiles = useAcpProfilesStore((state) => state.profiles);
  const modelId = useAcpSettingsStore((state) => state.modelId);
  const reasoningEffort = useAcpSettingsStore((state) => state.reasoningEffort);
  const queryClient = useQueryClient();
  const agentConfigured = Boolean(
    activeProfileId?.trim() &&
      profiles.some((profile) => profile.id === activeProfileId),
  );
  const metadataQuery = useQuery({
    queryKey: ["library-context-for-media", mediaPath ?? null],
    queryFn: () => getMediaMetadataContext(mediaPath as string),
    enabled: Boolean(mediaPath),
    retry: false,
    staleTime: 30_000,
  });
  const isIdentityPending = Boolean(mediaPath) && metadataQuery.isPending;
  const authoritativeIdentity = chapterEpisodeIdentityFromLibraryContext(
    metadataQuery.data,
  );
  const episodeIdentity = mediaPath && !isIdentityPending
    ? authoritativeIdentity ?? {
        kind: "legacy" as const,
        reason: metadataQuery.isError
          ? "context_lookup_failed" as const
          : metadataQuery.data === null
            ? "metadata_unavailable" as const
            : "metadata_incomplete" as const,
      }
    : null;
  const identityStatus: ChapterIdentityResolution = !mediaPath
    ? "unavailable"
    : isIdentityPending
      ? "pending"
      : episodeIdentity?.kind === "authoritative"
        ? "authoritative"
        : "legacy";
  const episodeKey = episodeIdentity?.kind === "authoritative"
    ? episodeIdentity.episodeStableId
    : mediaPath ?? null;
  const request: ChapterSegmentationRequest | null = mediaPath && episodeIdentity
    ? {
        mediaPath,
        episodeKey: episodeKey as string,
        episodeIdentity,
        profileId: activeProfileId,
        profiles: profilesHintFromStore(activeProfileId, profiles),
        modelId: modelId?.trim() || null,
        reasoningEffort: reasoningEffort?.trim() || null,
        subtitleChoiceId,
        positionMs,
        spoilerBoundary: "full_media",
      }
    : null;
  const queryKey = chapterSegmentationKey(
    episodeIdentity?.kind === "authoritative" ? null : mediaPath,
    episodeKey,
  );

  const statusQuery = useQuery({
    queryKey,
    queryFn: () => getChapterSegmentationStatus(request as ChapterSegmentationRequest),
    enabled: Boolean(request),
    retry: false,
    staleTime: 1_000,
    refetchInterval: (query) =>
      isPendingTask(query.state.data?.status) ? 1_000 : false,
  });

  const startMutation = useMutation({
    mutationFn: () => {
      if (!request) {
        return Promise.reject(new Error("当前没有可分段的媒体"));
      }
      return startChapterSegmentation(request);
    },
    onSuccess: (snapshot) => {
      queryClient.setQueryData(queryKey, snapshot);
      void queryClient.invalidateQueries({ queryKey });
    },
  });

  const snapshot = statusQuery.data;
  const liveProgress = useChapterProgressStore((state) =>
    snapshot?.taskKey ? state.byTaskKey[snapshot.taskKey] ?? null : null,
  );
  const pending =
    startMutation.isPending || isPendingTask(snapshot?.status);
  const status: ChapterSegmentationUiStatus = !request
    ? "unavailable"
    : pending
      ? "pending"
      : !agentConfigured &&
          (snapshot?.status === "pending" ||
            snapshot?.status === "validation_failure" ||
            snapshot?.status === "failed")
        ? "unavailable"
      : statusQuery.isError && !isMissingTaskError(statusQuery.error)
        ? "unavailable"
        : snapshot?.status === "succeeded"
          ? "completed"
          : snapshot?.status === "validation_failure"
            ? "validation_failure"
          : snapshot?.status === "failed"
            ? "failed"
            : "ready";
  const error =
    statusQuery.isError && !isMissingTaskError(statusQuery.error)
      ? statusQuery.error
      : startMutation.error;

  return {
    request,
    snapshot,
    liveProgress,
    status,
    isPending: pending,
    isIdentityPending,
    identityStatus,
    episodeIdentity,
    failureMessage: snapshot?.failureMessage ?? null,
    errorMessage: snapshot?.failureMessage ?? (error ? errorMessage(error) : null),
    validationSummary: snapshot?.validationSummary ?? null,
    attemptCount: snapshot?.attemptCount ?? 0,
    maxAttempts: snapshot?.maxAttempts ?? 0,
    canRetry: snapshot?.canRetry ?? false,
    retryAction: snapshot?.retryAction ?? null,
    agentConfigured: snapshot?.agentConfigured ?? agentConfigured,
    taskStatus: snapshot?.status ?? null,
    start: () => startMutation.mutate(),
    startAsync: () => startMutation.mutateAsync(),
    isStarting: startMutation.isPending,
  };
}

export type ChapterSegmentationController = ReturnType<
  typeof useChapterSegmentation
>;

export type { ChapterSegmentationSnapshot };
