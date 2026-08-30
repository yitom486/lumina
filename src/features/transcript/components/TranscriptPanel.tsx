import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { formatTime } from "@/lib/format";
import { usePlayerStore } from "@/features/player";

import {
  listSubtitleTracks,
  loadExternalTranscript,
  loadTranscript,
  pickExternalSubtitle,
} from "../api";
import type { Cue, SubtitleTrackInfo, Transcript } from "../types";

function trackLabel(track: SubtitleTrackInfo): string {
  const lang = track.language ?? "und";
  const codec = track.codecName ?? "sub";
  return `#${track.streamIndex} · ${lang} · ${codec}`;
}

function activeCueIndex(cues: Cue[], timeMs: number): number {
  return cues.findIndex((c) => timeMs >= c.startMs && timeMs < c.endMs);
}

export function TranscriptPanel() {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const seek = usePlayerStore((s) => s.seek);

  const mediaReady =
    Boolean(path) &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  const tracksQuery = useQuery({
    queryKey: ["subtitleTracks", path],
    queryFn: () => listSubtitleTracks(path as string),
    enabled: mediaReady,
    retry: false,
  });

  const [streamIndex, setStreamIndex] = useState<number | null>(null);
  const [external, setExternal] = useState<Transcript | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    setExternal(null);
    setLoadError(null);
    setStreamIndex(null);
  }, [path]);

  useEffect(() => {
    if (!tracksQuery.data?.length) return;
    if (streamIndex != null) return;
    const preferred =
      tracksQuery.data.find(
        (t) => t.codecName && !/pgs|dvd_subtitle|dvb_subtitle/i.test(t.codecName),
      ) ?? tracksQuery.data[0];
    setStreamIndex(preferred.streamIndex);
  }, [tracksQuery.data, streamIndex]);

  const transcriptQuery = useQuery({
    queryKey: ["transcript", path, streamIndex],
    queryFn: () => loadTranscript(path as string, streamIndex as number),
    enabled: mediaReady && streamIndex != null && !external,
    retry: false,
  });

  const transcript = external ?? transcriptQuery.data ?? null;
  const activeIndex = useMemo(
    () => (transcript ? activeCueIndex(transcript.cues, currentTimeMs) : -1),
    [transcript, currentTimeMs],
  );

  async function handleOpenExternal() {
    setLoadError(null);
    try {
      const file = await pickExternalSubtitle();
      if (!file) return;
      const data = await loadExternalTranscript(file);
      setExternal(data);
    } catch (error) {
      setLoadError(
        typeof error === "object" && error && "message" in error
          ? String((error as { message: string }).message)
          : String(error),
      );
    }
  }

  if (!mediaReady) {
    return (
      <section className="border-t border-border px-6 py-3 text-sm text-muted-foreground">
        Open a video to load subtitle transcript.
      </section>
    );
  }

  const errorText =
    loadError ??
    (transcriptQuery.isError
      ? String(
          (transcriptQuery.error as { message?: string }).message ??
            transcriptQuery.error,
        )
      : null) ??
    (tracksQuery.isError
      ? String(
          (tracksQuery.error as { message?: string }).message ?? tracksQuery.error,
        )
      : null);

  return (
    <section className="flex max-h-64 flex-col border-t border-border">
      <div className="flex flex-wrap items-center gap-3 px-6 py-2 text-sm">
        <span className="font-medium">Transcript</span>
        {tracksQuery.data && tracksQuery.data.length > 0 ? (
          <select
            className="rounded border border-border bg-background px-2 py-1"
            value={streamIndex ?? ""}
            disabled={Boolean(external)}
            onChange={(e) => {
              setExternal(null);
              setStreamIndex(Number(e.target.value));
            }}
            aria-label="Subtitle track"
          >
            {tracksQuery.data.map((track) => (
              <option key={track.streamIndex} value={track.streamIndex}>
                {trackLabel(track)}
              </option>
            ))}
          </select>
        ) : (
          <span className="text-muted-foreground">No embedded text tracks</span>
        )}
        <button
          type="button"
          className="rounded border border-border px-2 py-1 hover:bg-black/5"
          onClick={() => void handleOpenExternal()}
        >
          Open .srt/.vtt/.ass
        </button>
        {external ? (
          <button
            type="button"
            className="text-muted-foreground underline"
            onClick={() => setExternal(null)}
          >
            Use embedded
          </button>
        ) : null}
      </div>

      {transcriptQuery.isLoading && !external ? (
        <p className="px-6 pb-3 text-sm text-muted-foreground">
          Extracting subtitle…
        </p>
      ) : null}

      {errorText && !transcript ? (
        <p className="px-6 pb-3 text-sm text-red-600">{errorText}</p>
      ) : null}

      {transcript ? (
        <ul className="min-h-0 flex-1 space-y-1 overflow-y-auto px-4 pb-3">
          {transcript.cues.map((cue, i) => {
            const active = i === activeIndex;
            return (
              <li key={cue.index}>
                <button
                  type="button"
                  className={`w-full rounded px-2 py-1.5 text-left text-sm ${
                    active
                      ? "bg-black/10 font-medium"
                      : "hover:bg-black/5 text-muted-foreground"
                  }`}
                  onClick={() => void seek(cue.startMs)}
                >
                  <span className="mr-2 tabular-nums text-xs opacity-70">
                    {formatTime(cue.startMs)}
                  </span>
                  {cue.text}
                </button>
              </li>
            );
          })}
        </ul>
      ) : null}
    </section>
  );
}
