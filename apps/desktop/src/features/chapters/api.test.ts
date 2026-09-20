import { beforeEach, describe, expect, it, vi } from "vitest";

import { invoke } from "@tauri-apps/api/core";

import {
  chapterEpisodeIdentityFromLibraryContext,
  getChapterSegmentationStatus,
  parseDraftChapters,
  parseGeneratedChapters,
  startChapterSegmentation,
  type ChapterSegmentationSnapshot,
} from "./api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const request = {
  mediaPath: "C:\\videos\\episode-01.mkv",
  episodeKey: "C:\\videos\\episode-01.mkv",
  episodeIdentity: { kind: "legacy" as const, reason: "metadata_unavailable" as const },
};

const snapshot: ChapterSegmentationSnapshot = {
  id: 1,
  taskKey: "chapter-segmentation:C:\\videos\\episode-01.mkv",
  taskType: "chapter_segmentation",
  episodeId: null,
  chapterId: null,
  episodeIdentity: { kind: "legacy", reason: "metadata_unavailable" },
  status: "pending",
  sessionId: null,
  promptVersion: "1.0",
  outputContractVersion: "chapter_tool_workflow.v1",
  attemptCount: 0,
  retryCount: 0,
  maxAttempts: 3,
  failureCode: null,
  failureMessage: null,
  validationSummary: null,
  canRetry: false,
  retryAction: null,
  agentConfigured: true,
  outputJson: null,
  draftChapters: [],
  createdAtMs: 1,
  updatedAtMs: 1,
};

beforeEach(() => {
  vi.mocked(invoke).mockResolvedValue(snapshot);
});

describe("chapter segmentation API", () => {
  it("invokes the start command with the backend request envelope", async () => {
    await expect(startChapterSegmentation(request)).resolves.toEqual(snapshot);

    expect(invoke).toHaveBeenCalledWith("chapter_segmentation_start", {
      request,
    });
  });

  it("invokes the status command with the same stable request", async () => {
    await expect(getChapterSegmentationStatus(request)).resolves.toEqual(snapshot);

    expect(invoke).toHaveBeenCalledWith("chapter_segmentation_status", {
      request,
    });
  });

  it("parses only succeeded structured chapter output", () => {
    expect(
      parseGeneratedChapters({
        ...snapshot,
        status: "succeeded",
        outputJson: JSON.stringify({
          chapters: [
            { id: "intro", title: "开场", start_ms: 0, end_ms: 1_200 },
            { id: "invalid", title: "坏数据", start_ms: 4_000, end_ms: 2_000 },
          ],
        }),
      }),
    ).toEqual([{ id: 1, title: "开场", startMs: 0, endMs: 1_200 }]);
    expect(parseGeneratedChapters(snapshot)).toEqual([]);
  });

  it("projects durable draft chapters before task success", () => {
    const drafts = [
      {
        id: 12,
        stableId: "chapter-01",
        startMs: 0,
        endMs: 30_000,
        title: "开场",
        mainline: null,
        status: "waiting_evidence" as const,
        updatedAtMs: 2,
      },
    ];
    expect(
      parseDraftChapters({
        ...snapshot,
        status: "running",
        draftChapters: drafts,
      }),
    ).toEqual(drafts);
  });

  it("derives authoritative identity only from complete matched TV episode metadata", () => {
    expect(
      chapterEpisodeIdentityFromLibraryContext({
        mediaPath: "C:\\videos\\episode-01.mkv",
        group: {
          kind: "series",
          tmdbId: 123,
          title: "示例剧集",
        },
        item: {
          kind: "episode",
          tmdbId: 456,
          seriesTmdbId: 123,
          season: 2,
          episode: 7,
          title: "第七集",
        },
      } as never),
    ).toEqual({
      kind: "authoritative",
      seriesStableId: "tmdb:tv:123",
      episodeStableId: "s02e07",
      season: 2,
      episode: 7,
      seriesTitle: "示例剧集",
      title: "第七集",
    });
  });

  it("returns no identity for incomplete metadata instead of guessing from the path", () => {
    expect(
      chapterEpisodeIdentityFromLibraryContext({
        mediaPath: "C:\\videos\\S02E07.mkv",
        group: {
          kind: "series",
          tmdbId: 123,
          title: "示例剧集",
        },
        item: null,
      } as never),
    ).toBeNull();
    expect(
      chapterEpisodeIdentityFromLibraryContext({
        mediaPath: "C:\\videos\\S02E07.mkv",
        group: {
          kind: "series",
          tmdbId: 123,
          title: "示例剧集",
        },
        item: {
          kind: "episode",
          tmdbId: 456,
          season: null,
          episode: 7,
          title: "第七集",
        },
      } as never),
    ).toBeNull();
  });

  it("keeps authoritative identity stable when the media path changes", () => {
    const context = {
      mediaPath: "D:\\old\\episode.mkv",
      group: { kind: "series", tmdbId: 321, title: "剧集" },
      item: {
        kind: "episode",
        tmdbId: 654,
        seriesTmdbId: 321,
        season: 1,
        episode: 3,
        title: "第三集",
      },
    } as never;
    const identity = chapterEpisodeIdentityFromLibraryContext(context);
    expect(identity?.episodeStableId).toBe("s01e03");
    expect(identity?.seriesStableId).toBe("tmdb:tv:321");
  });
});
