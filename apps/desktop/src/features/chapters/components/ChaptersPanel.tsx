import { useQuery } from "@tanstack/react-query";

import { useMediaInfoQuery } from "@/features/media";
import { usePlayerStore } from "@/features/player";
import { ytdlResolveKey } from "@lumina/query-keys";
import { getCachedYtdlResolve } from "@/features/ytdl";
import { formatTime } from "@/lib/format";
import type { MediaChapter } from "@lumina/contracts";
import { parseGeneratedChapters } from "../api";
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
  const generatedChapters = parseGeneratedChapters(segmentation.snapshot);
  if (chapters.length === 0) {
    if (generatedChapters.length > 0) {
      return (
        <ChapterList
          chapters={generatedChapters}
          positionMs={positionMs}
          seek={seek}
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
          任务已保存，章节 Agent 正在独立执行；不会写入自由聊天，也不会自动重复启动。
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
