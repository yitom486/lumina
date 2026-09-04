import { errorMessage, formatTime } from "@/lib/format";
import { Separator } from "@/components/ui/separator";
import { usePlayerStore } from "@/features/player";

import { useMediaInfoQuery } from "../hooks/useMediaInfoQuery";
import type { MediaStream } from "../types";

function formatBitrate(bps?: number | null): string | null {
  if (bps == null || bps <= 0) return null;
  if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(1)} Mbps`;
  if (bps >= 1_000) return `${(bps / 1_000).toFixed(0)} kbps`;
  return `${bps} bps`;
}

function shortStream(stream: MediaStream): string {
  if (stream.kind === "Video") {
    const size =
      stream.width && stream.height
        ? `${stream.width}×${stream.height}`
        : "video";
    return `${size} · ${stream.codecName ?? "?"}`;
  }
  if (stream.kind === "Audio") {
    return `${stream.codecName ?? "audio"}${stream.language ? ` · ${stream.language}` : ""}`;
  }
  return stream.codecName ?? stream.kind;
}

/** Compact media summary for the reader sidebar. */
export function MediaInfoPanel() {
  const sourceKind = usePlayerStore((s) => s.sourceKind);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const query = useMediaInfoQuery();

  if (sourceKind === "remote" || (currentFile?.startsWith("http") ?? false)) {
    return (
      <div className="space-y-1 px-3 py-2 text-xs text-muted-foreground">
        <p className="font-medium text-foreground">在线视频</p>
        <p className="break-all">{currentFile}</p>
        <p>媒体信息由在线解析提供；容器探测仅用于本地文件。</p>
        <Separator className="mt-2" />
      </div>
    );
  }

  if (!query.isEnabled) {
    return (
      <div className="px-3 py-2 text-xs text-muted-foreground">
        打开视频后显示媒体信息
      </div>
    );
  }

  if (query.isLoading) {
    return (
      <div className="px-3 py-2 text-xs text-muted-foreground">探测中…</div>
    );
  }

  if (query.isError) {
    return (
      <div className="px-3 py-2 text-xs text-destructive">
        {errorMessage(query.error)}
      </div>
    );
  }

  const info = query.data;
  if (!info) return null;

  const video = info.streams.find((s: MediaStream) => s.kind === "Video");
  const audio = info.streams.find((s: MediaStream) => s.kind === "Audio");
  const bitrate = formatBitrate(info.bitRate);

  return (
    <div className="space-y-1 px-3 py-2 text-xs text-muted-foreground">
      <p className="font-medium text-foreground">
        {info.formatLongName ?? info.formatName ?? "unknown"}
        {info.durationMs != null ? ` · ${formatTime(info.durationMs)}` : ""}
        {bitrate ? ` · ${bitrate}` : ""}
      </p>
      {video ? <p>视频：{shortStream(video)}</p> : null}
      {audio ? <p>音频：{shortStream(audio)}</p> : null}
      <Separator className="mt-2" />
    </div>
  );
}
