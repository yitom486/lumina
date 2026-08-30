import { formatTime } from "@/lib/format";

import { useMediaInfoQuery } from "../hooks/useMediaInfoQuery";
import type { MediaStream } from "../types";

function fileName(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

function formatBitrate(bps?: number | null): string | null {
  if (bps == null || bps <= 0) return null;
  if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(2)} Mbps`;
  if (bps >= 1_000) return `${(bps / 1_000).toFixed(0)} kbps`;
  return `${bps} bps`;
}

function describeStream(stream: MediaStream): string {
  if (stream.kind === "Video") {
    const size =
      stream.width && stream.height
        ? `${stream.width}×${stream.height}`
        : "video";
    const fps = stream.frameRate
      ? ` @ ${stream.frameRate.toFixed(2)} fps`
      : "";
    const codec = stream.codecName ?? "?";
    return `${size}${fps} · ${codec}`;
  }
  if (stream.kind === "Audio") {
    const rate = stream.sampleRate ? `${stream.sampleRate} Hz` : "audio";
    const ch = stream.channels ? ` · ${stream.channels}ch` : "";
    const codec = stream.codecName ?? "?";
    const lang = stream.language ? ` · ${stream.language}` : "";
    return `${rate}${ch} · ${codec}${lang}`;
  }
  if (stream.kind === "Subtitle") {
    const codec = stream.codecName ?? "subtitle";
    const lang = stream.language ? ` · ${stream.language}` : "";
    return `${codec}${lang}`;
  }
  return stream.codecName ?? stream.kind;
}

export function MediaInfoPanel() {
  const query = useMediaInfoQuery();

  if (!query.isEnabled) {
    return (
      <aside className="border-t border-border px-6 py-3 text-sm text-muted-foreground">
        Open a video to inspect container and streams (ffprobe).
      </aside>
    );
  }

  if (query.isLoading) {
    return (
      <aside className="border-t border-border px-6 py-3 text-sm text-muted-foreground">
        Inspecting media…
      </aside>
    );
  }

  if (query.isError) {
    const err = query.error as { message?: string; code?: string };
    return (
      <aside className="border-t border-border px-6 py-3 text-sm text-red-600">
        Inspect failed
        {err.code ? ` [${err.code}]` : ""}: {err.message ?? String(query.error)}
      </aside>
    );
  }

  const info = query.data;
  if (!info) return null;

  const video = info.streams.find((s: MediaStream) => s.kind === "Video");
  const audio = info.streams.find((s: MediaStream) => s.kind === "Audio");
  const bitrate = formatBitrate(info.bitRate);

  return (
    <aside className="border-t border-border px-6 py-3 text-sm">
      <p className="font-medium">{fileName(info.path)}</p>
      <p className="mt-1 text-muted-foreground">
        {info.formatLongName ?? info.formatName ?? "unknown format"}
        {info.durationMs != null ? ` · ${formatTime(info.durationMs)}` : ""}
        {bitrate ? ` · ${bitrate}` : ""}
        {info.sizeBytes != null
          ? ` · ${(info.sizeBytes / (1024 * 1024)).toFixed(1)} MB`
          : ""}
      </p>
      <ul className="mt-2 space-y-1 text-muted-foreground">
        {video ? <li>Video: {describeStream(video)}</li> : null}
        {audio ? <li>Audio: {describeStream(audio)}</li> : null}
        <li>
          Streams: {info.streams.length}
          {info.streams.some((s: MediaStream) => s.kind === "Subtitle")
            ? " (includes subtitle)"
            : ""}
        </li>
      </ul>
    </aside>
  );
}
