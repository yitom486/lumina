import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { useMediaInfoQuery } from "@/features/media";
import { usePlayerStore } from "@/features/player";
import { ytdlResolveKey } from "@lumina/query-keys";
import { getCachedYtdlResolve } from "@/features/ytdl";
import { formatTime } from "@/lib/format";
import type { MediaChapter } from "@lumina/contracts";
import { chapterAssetKey, getChapterAsset, parseDraftChapters } from "../api";
import type { ChapterAssetData, ChapterDetailSnapshot, ChapterDraftSnapshot } from "../api";
import { useChapterSegmentation } from "../hooks/useChapterSegmentation";

export type AiSegmentationStatus =
  | "unavailable"
  | "ready"
  | "pending"
  | "validation_failure"
  | "completed"
  | "failed";

export type ChaptersPanelProps = {
  /** Starts the user-requested AI chapter segmentation job when wired by the app. */
  onStartAiSegmentation?: () => void;
  /** The orchestration layer owns this state; the panel never starts it on mount. */
  aiSegmentationStatus?: AiSegmentationStatus;
  /** Safe business failure summary supplied by the durable chapter task. */
  failureMessage?: string | null;
  validationSummary?: string | null;
  attemptCount?: number;
  maxAttempts?: number;
  canRetry?: boolean;
  retryAction?: "retry" | "configure_agent" | null;
  agentConfigured?: boolean;
  taskStatus?: string | null;
  progressMessage?: string | null;
};

export function ChaptersPanel({
  onStartAiSegmentation,
  aiSegmentationStatus,
}: ChaptersPanelProps) {
  const mediaPath = usePlayerStore((s) => s.currentFile);
  const sourceKind = usePlayerStore((s) => s.sourceKind);
  const seek = usePlayerStore((s) => s.seek);
  const positionMs = usePlayerStore((s) => s.currentTimeMs);
  const segmentation = useChapterSegmentation();
  const effectiveSegmentationStatus =
    aiSegmentationStatus ?? segmentation.status;
  const startSegmentation = onStartAiSegmentation ?? segmentation.start;
  const isRemote =
    sourceKind === "remote" || (mediaPath?.startsWith("http") ?? false);
  const { data, isLoading, error } = useMediaInfoQuery();
  // Online chapters come from the cached ytdl resolve result shared with the
  // online/transcript/ACP surfaces (same query key, no refetch, no fresh resolve).
  const onlineQuery = useQuery({
    queryKey: ytdlResolveKey(mediaPath),
    queryFn: () => getCachedYtdlResolve(mediaPath as string),
    enabled: Boolean(isRemote && mediaPath),
    retry: false,
    staleTime: Infinity,
  });

  if (!mediaPath) {
    return (
      <div className="p-3 text-xs text-muted-foreground">打开含章节元数据的视频后显示。</div>
    );
  }

  if (isRemote) {
    const onlineChapters = onlineQuery.data?.chapters ?? [];
    if (onlineQuery.isLoading) {
      return <div className="p-3 text-xs text-muted-foreground">正在读取在线章节…</div>;
    }
    if (onlineChapters.length === 0) {
      const draftChapters = parseDraftChapters(segmentation.snapshot);
      if (draftChapters.length > 0) {
        return (
          <ChapterDraftList
            chapters={draftChapters}
            positionMs={positionMs}
            seek={seek}
            progressMessage={segmentation.liveProgress?.message}
          />
        );
      }
      return (
        <EmptyChaptersState
          onStartAiSegmentation={startSegmentation}
          aiSegmentationStatus={effectiveSegmentationStatus}
          errorMessage={onStartAiSegmentation ? null : segmentation.errorMessage}
          failureMessage={onStartAiSegmentation ? null : segmentation.failureMessage}
          validationSummary={onStartAiSegmentation ? null : segmentation.validationSummary}
          attemptCount={onStartAiSegmentation ? undefined : segmentation.attemptCount}
          maxAttempts={onStartAiSegmentation ? undefined : segmentation.maxAttempts}
          canRetry={onStartAiSegmentation ? undefined : segmentation.canRetry}
          retryAction={onStartAiSegmentation ? undefined : segmentation.retryAction}
          agentConfigured={onStartAiSegmentation ? undefined : segmentation.agentConfigured}
          taskStatus={onStartAiSegmentation ? undefined : segmentation.taskStatus}
          progressMessage={onStartAiSegmentation ? null : segmentation.liveProgress?.message}
          description="该在线视频暂无真实章节。"
        />
      );
    }
    return (
      <ChapterList chapters={onlineChapters} positionMs={positionMs} seek={seek} />
    );
  }

  if (isLoading) {
    return <div className="p-3 text-xs text-muted-foreground">正在探测章节…</div>;
  }

  if (error) {
    return (
      <div className="p-3 text-xs text-muted-foreground">无法读取媒体信息（章节依赖探测）。</div>
    );
  }

  const chapters = data?.chapters ?? [];
  const draftChapters = parseDraftChapters(segmentation.snapshot);
  if (chapters.length === 0) {
    if (draftChapters.length > 0) {
      return (
        <ChapterDraftList
          chapters={draftChapters}
          positionMs={positionMs}
          seek={seek}
          progressMessage={segmentation.liveProgress?.message}
        />
      );
    }
    return (
      <EmptyChaptersState
        onStartAiSegmentation={startSegmentation}
        aiSegmentationStatus={effectiveSegmentationStatus}
        errorMessage={onStartAiSegmentation ? null : segmentation.errorMessage}
        failureMessage={onStartAiSegmentation ? null : segmentation.failureMessage}
        validationSummary={onStartAiSegmentation ? null : segmentation.validationSummary}
        attemptCount={onStartAiSegmentation ? undefined : segmentation.attemptCount}
        maxAttempts={onStartAiSegmentation ? undefined : segmentation.maxAttempts}
        canRetry={onStartAiSegmentation ? undefined : segmentation.canRetry}
        retryAction={onStartAiSegmentation ? undefined : segmentation.retryAction}
        agentConfigured={onStartAiSegmentation ? undefined : segmentation.agentConfigured}
        taskStatus={onStartAiSegmentation ? undefined : segmentation.taskStatus}
        progressMessage={onStartAiSegmentation ? null : segmentation.liveProgress?.message}
        description="该文件暂无容器章节。"
      />
    );
  }

  return (
    <ChapterList chapters={chapters} positionMs={positionMs} seek={seek} />
  );
}

function EmptyChaptersState({
  description,
  onStartAiSegmentation,
  aiSegmentationStatus,
  errorMessage,
  failureMessage,
  validationSummary,
  attemptCount,
  maxAttempts,
  canRetry = false,
  retryAction,
  agentConfigured = true,
  taskStatus,
  progressMessage,
}: ChaptersPanelProps & { description: string; errorMessage?: string | null }) {
  const retryableFailure =
    (aiSegmentationStatus === "validation_failure" ||
      aiSegmentationStatus === "failed") &&
    canRetry;
  const needsAgentConfiguration =
    retryAction === "configure_agent" || !agentConfigured;
  const canStartAiSegmentation =
    Boolean(onStartAiSegmentation) &&
    agentConfigured &&
    (aiSegmentationStatus === "ready" || retryableFailure);
  const handleStartAiSegmentation = () => {
    if (canStartAiSegmentation) {
      onStartAiSegmentation?.();
    }
  };

  return (
    <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3 text-xs">
      <div className="space-y-1 text-muted-foreground">
        <p>{description}</p>
        <p>点击后会创建独立章节 Agent 任务，结合字幕与按需画面生成语义章节。</p>
      </div>
      <button
        type="button"
        className="rounded-md bg-primary px-3 py-1.5 text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50"
        disabled={!canStartAiSegmentation}
        onClick={handleStartAiSegmentation}
      >
        {aiSegmentationStatus === "pending"
          ? taskStatus === "running"
            ? "正在分析字幕与画面…"
            : "已排队，等待执行…"
          : aiSegmentationStatus === "completed"
            ? "AI 分段已完成，等待章节写入…"
            : retryableFailure
              ? "再次尝试"
              : aiSegmentationStatus === "validation_failure" ||
                  aiSegmentationStatus === "failed"
                ? "AI 分段失败"
              : "开始 AI 分段"}
      </button>
      {needsAgentConfiguration &&
      (aiSegmentationStatus === "unavailable" ||
        aiSegmentationStatus === "pending" ||
        aiSegmentationStatus === "validation_failure" ||
        aiSegmentationStatus === "failed") ? (
        <p className="text-[11px] text-muted-foreground">
          尚未配置可用的 AI Agent，请先在设置中完成配置后再开始章节分析。
        </p>
      ) : null}
      {failureMessage ? <p className="text-destructive">{failureMessage}</p> : null}
      {validationSummary ? (
        <p className="text-[11px] text-muted-foreground">{validationSummary}</p>
      ) : null}
      {(aiSegmentationStatus === "validation_failure" ||
        aiSegmentationStatus === "failed") &&
      agentConfigured ? (
        <div className="space-y-1 text-[11px] text-muted-foreground">
          {typeof attemptCount === "number" && typeof maxAttempts === "number" ? (
            <p>
              已尝试 {attemptCount} / {maxAttempts} 次
              {maxAttempts > 0 && attemptCount >= maxAttempts
                ? "，已达到尝试上限。"
                : "。"}
            </p>
          ) : null}
          {retryableFailure && retryAction === "retry" ? (
            <p>再次尝试会创建新的独立章节 Agent 会话，不会写入自由聊天。</p>
          ) : null}
          {!retryableFailure && maxAttempts && attemptCount && attemptCount >= maxAttempts ? (
            <p>已达到自动校验和作业尝试上限，请检查字幕、画面证据或 Agent 设置后再处理。</p>
          ) : null}
        </div>
      ) : null}
      {aiSegmentationStatus === "pending" && agentConfigured ? (
        <p className="text-[11px] text-muted-foreground">
          {progressMessage ??
            "任务已保存，章节 Agent 正在独立执行；不会写入自由聊天，也不会自动重复启动。"}
        </p>
      ) : null}
      {aiSegmentationStatus === "completed" ? (
        <p className="text-[11px] text-muted-foreground">
          结果已通过校验并保存，章节列表将在任务投影完成后显示。
        </p>
      ) : null}
      {aiSegmentationStatus === "unavailable" && agentConfigured && errorMessage ? (
        <p className="text-[11px] text-muted-foreground">{errorMessage}</p>
      ) : null}
    </div>
  );
}

function ChapterList({
  chapters,
  positionMs,
  seek,
}: {
  chapters: MediaChapter[];
  positionMs: number;
  seek: (ms: number) => unknown;
}) {
  return (
    <div className="min-h-0 flex-1 space-y-1 overflow-auto p-3">
      {chapters.map((chapter, index) => {
        const active =
          positionMs >= chapter.startMs &&
          (chapter.endMs == null || positionMs < chapter.endMs);
        return (
          <button
            key={`${chapter.id}-${chapter.startMs}`}
            type="button"
            className={
              active
                ? "flex w-full items-start gap-2 rounded-md bg-accent px-2 py-1.5 text-left text-xs"
                : "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted"
            }
            onClick={() => void seek(chapter.startMs)}
          >
            <span className="shrink-0 font-mono text-primary">
              {formatTime(chapter.startMs)}
            </span>
            <span className="min-w-0 flex-1">
              {chapter.title?.trim() || `章节 ${index + 1}`}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function ChapterDraftList({
  chapters,
  positionMs,
  seek,
  progressMessage,
}: {
  chapters: ChapterDraftSnapshot[];
  positionMs: number;
  seek: (ms: number) => unknown;
  progressMessage?: string | null;
}) {
  const [selectedChapterId, setSelectedChapterId] = useState<number | null>(
    chapters[0]?.id ?? null,
  );
  const selectedChapter =
    chapters.find((chapter) => chapter.id === selectedChapterId) ?? chapters[0];

  return (
    <div className="min-h-0 flex-1 space-y-2 overflow-auto p-3">
      <div className="text-xs text-muted-foreground">
        AI 章节草稿 · 已从任务大纲持久化
      </div>
      {progressMessage ? (
        <div className="rounded-md bg-muted px-2 py-1.5 text-[11px] text-muted-foreground">
          {progressMessage}
        </div>
      ) : null}
      {chapters.map((chapter, index) => {
        const active = positionMs >= chapter.startMs && positionMs < chapter.endMs;
        return (
          <button
            key={`${chapter.id}-${chapter.updatedAtMs}`}
            type="button"
            className={
              active
                ? "flex w-full items-start gap-2 rounded-md bg-accent px-2 py-1.5 text-left text-xs"
                : "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted"
            }
            aria-pressed={selectedChapter?.id === chapter.id}
            onClick={() => {
              setSelectedChapterId(chapter.id);
              void seek(chapter.startMs);
            }}
          >
            <span className="shrink-0 font-mono text-primary">
              {formatTime(chapter.startMs)}
            </span>
            <span className="min-w-0 flex-1 space-y-0.5">
              <span className="block">
                {chapter.title?.trim() || `章节 ${index + 1}`}
              </span>
              <span className="block text-[11px] text-muted-foreground">
                {chapterDraftStatusLabel(chapter.status)}
              </span>
            </span>
          </button>
        );
      })}
      {selectedChapter ? (
        <ChapterDetailView detail={selectedChapter.detail} />
      ) : null}
    </div>
  );
}

function ChapterDetailView({
  detail,
}: {
  detail?: ChapterDetailSnapshot | null;
}) {
  const coverAssetRef = detail?.coverAssetRef ?? null;
  const coverQuery = useQuery({
    queryKey: chapterAssetKey(coverAssetRef),
    queryFn: () => getChapterAsset(coverAssetRef as string),
    enabled: Boolean(coverAssetRef),
    staleTime: Infinity,
    retry: false,
  });

  if (!detail) {
    return (
      <section className="space-y-1 rounded-lg border border-border bg-card p-3 text-xs">
        <h3 className="font-medium text-foreground">章节详情</h3>
        <p className="text-muted-foreground">详情尚未生成，当前仅显示章节大纲。</p>
      </section>
    );
  }

  const evidenceCount = detail.evidenceAssetRefs.length;
  const metadata = [
    detail.revisionNumber ? `第 ${detail.revisionNumber} 版` : null,
    detail.revisionStatus === "accepted" ? "已通过校验" : null,
    evidenceCount > 0 ? `${evidenceCount} 个画面证据` : null,
  ].filter(Boolean);

  return (
    <section className="space-y-3 rounded-lg border border-border bg-card p-3 text-xs">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3 className="font-medium text-foreground">章节详情</h3>
          <p className="mt-1 text-[11px] text-muted-foreground">
            {metadata.length > 0 ? metadata.join(" · ") : "来自已保存的章节内容"}
          </p>
        </div>
        {detail.coverAssetRef ? (
          <span className="rounded-full bg-muted px-2 py-1 text-[11px] text-muted-foreground">
            {coverQuery.data ? "代表画面" : "含代表画面"}
          </span>
        ) : null}
      </div>

      <ChapterAssetPreview
        asset={coverQuery.data}
        loading={coverQuery.isLoading}
        unavailable={Boolean(coverAssetRef) && Boolean(coverQuery.error || coverQuery.data === null)}
      />

      <ChapterDetailSection title="前情提要" value={detail.recap} empty="暂无前情提要" />
      <ChapterDetailList
        title="本章不剧透的观看重点"
        values={detail.watchPoints}
        empty="暂无观看重点"
      />
      <ChapterDetailSection title="本章剧情" value={detail.mainline} empty="暂无剧情详情" />
      <ChapterDetailSection title="后续看点" value={detail.outlook} empty="暂无后续看点" />
      <ChapterDetailList
        title="可以留意的问题"
        values={detail.questions}
        empty="暂无问题候选"
      />
    </section>
  );
}

function ChapterAssetPreview({
  asset,
  loading,
  unavailable,
}: {
  asset?: ChapterAssetData | null;
  loading: boolean;
  unavailable: boolean;
}) {
  if (asset) {
    return (
      <div className="overflow-hidden rounded-md border border-border bg-muted">
        <img
          src={`data:${asset.mime};base64,${asset.data}`}
          alt="章节代表画面"
          className="max-h-48 w-full object-cover"
        />
      </div>
    );
  }
  return (
    <div
      role="img"
      aria-label="章节代表画面占位"
      className="flex min-h-24 items-center justify-center rounded-md border border-dashed border-border bg-muted px-3 text-center text-[11px] text-muted-foreground"
    >
      {loading ? "正在读取代表画面…" : unavailable ? "代表画面暂不可用" : "暂无代表画面"}
    </div>
  );
}

function ChapterDetailSection({
  title,
  value,
  empty,
}: {
  title: string;
  value: string | null;
  empty: string;
}) {
  return (
    <div className="space-y-1">
      <h4 className="font-medium text-foreground">{title}</h4>
      <p className="whitespace-pre-wrap leading-5 text-muted-foreground">{value?.trim() || empty}</p>
    </div>
  );
}

function ChapterDetailList({
  title,
  values,
  empty,
}: {
  title: string;
  values: string[];
  empty: string;
}) {
  return (
    <div className="space-y-1">
      <h4 className="font-medium text-foreground">{title}</h4>
      {values.length > 0 ? (
        <ul className="space-y-1 text-muted-foreground">
          {values.map((value, index) => (
            <li key={`${value}-${index}`} className="flex gap-2 leading-5">
              <span className="text-primary" aria-hidden="true">
                ·
              </span>
              <span>{value}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-muted-foreground">{empty}</p>
      )}
    </div>
  );
}

function chapterDraftStatusLabel(status: ChapterDraftSnapshot["status"]): string {
  switch (status) {
    case "waiting_evidence":
      return "等待取证";
    case "analyzing":
      return "分析中";
    case "generated":
      return "已生成";
    case "validation_failed":
      return "校验失败";
  }
}
