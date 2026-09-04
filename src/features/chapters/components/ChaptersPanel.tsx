import { useMediaInfoQuery } from "@/features/media";
import { usePlayerStore } from "@/features/player";
import { formatTime } from "@/lib/format";

export function ChaptersPanel() {
  const mediaPath = usePlayerStore((s) => s.currentFile);
  const sourceKind = usePlayerStore((s) => s.sourceKind);
  const seek = usePlayerStore((s) => s.seek);
  const positionMs = usePlayerStore((s) => s.currentTimeMs);
  const { data, isLoading, error } = useMediaInfoQuery();

  if (!mediaPath) {
    return (
      <div className="p-3 text-xs text-muted-foreground">打开含章节元数据的视频后显示。</div>
    );
  }

  if (sourceKind === "remote" || mediaPath.startsWith("http")) {
    return (
      <div className="p-3 text-xs text-muted-foreground">
        在线视频的章节将由解析结果提供（后续版本）；本地容器章节探测不适用于网页链接。
      </div>
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
  if (chapters.length === 0) {
    return (
      <div className="p-3 text-xs text-muted-foreground">
        该文件没有容器章节元数据。一般视频不会自动生成断点。
      </div>
    );
  }

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
