import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { formatTime } from "@/lib/format";
import { usePlayerStore } from "@/features/player";

import { listSubtitleChoices, loadSubtitleChoice } from "../api";
import type { Cue, SubtitleChoice } from "../types";

function activeCueIndex(cues: Cue[], timeMs: number): number {
  return cues.findIndex((c) => timeMs >= c.startMs && timeMs < c.endMs);
}

function pickDefaultChoice(choices: SubtitleChoice[]): string | null {
  const text = choices.find((c) => c.supported);
  if (text) return text.id;
  // Bitmap-only: still selectable for on-video display via mpv.
  return choices[0]?.id ?? null;
}

async function applyChoiceToPlayer(
  choice: SubtitleChoice | undefined,
  setSubtitle: (args: {
    source: "Embedded" | "Sidecar" | "None";
    streamIndex?: number | null;
    externalPath?: string | null;
  }) => Promise<void>,
) {
  if (!choice) {
    await setSubtitle({ source: "None" });
    return;
  }
  if (choice.source === "Embedded") {
    await setSubtitle({
      source: "Embedded",
      streamIndex: choice.streamIndex,
    });
    return;
  }
  await setSubtitle({
    source: "Sidecar",
    externalPath: choice.externalPath,
  });
}

export function TranscriptPanel() {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const seek = usePlayerStore((s) => s.seek);
  const setSubtitle = usePlayerStore((s) => s.setSubtitle);

  const mediaReady =
    Boolean(path) &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  const choicesQuery = useQuery({
    queryKey: ["subtitleChoices", path],
    queryFn: () => listSubtitleChoices(path as string),
    enabled: mediaReady,
    retry: false,
  });

  const [choiceId, setChoiceId] = useState<string | null>(null);

  useEffect(() => {
    setChoiceId(null);
  }, [path]);

  useEffect(() => {
    if (!choicesQuery.data?.length) return;
    if (choiceId != null) return;
    setChoiceId(pickDefaultChoice(choicesQuery.data));
  }, [choicesQuery.data, choiceId]);

  const selected = choicesQuery.data?.find((c) => c.id === choiceId);

  // Always push selection to mpv so subtitles appear on the video surface.
  useEffect(() => {
    if (!mediaReady || !choiceId) return;
    const choice = choicesQuery.data?.find((c) => c.id === choiceId);
    void applyChoiceToPlayer(choice, setSubtitle);
  }, [mediaReady, choiceId, choicesQuery.data, setSubtitle]);

  const canLoadTranscript = Boolean(choiceId && selected?.supported);

  const transcriptQuery = useQuery({
    queryKey: ["transcript", path, choiceId],
    queryFn: () => loadSubtitleChoice(path as string, choiceId as string),
    enabled: mediaReady && canLoadTranscript,
    retry: false,
  });

  const transcript = transcriptQuery.data ?? null;
  const activeIndex = useMemo(
    () => (transcript ? activeCueIndex(transcript.cues, currentTimeMs) : -1),
    [transcript, currentTimeMs],
  );

  // Keep active cue visible while playing.
  useEffect(() => {
    if (activeIndex < 0) return;
    const el = document.getElementById(`cue-${activeIndex}`);
    el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [activeIndex]);

  if (!mediaReady) {
    return (
      <section className="border-t border-border px-6 py-3 text-sm text-muted-foreground">
        Open a video to choose a subtitle track.
      </section>
    );
  }

  const choices = choicesQuery.data ?? [];
  const errorText = transcriptQuery.isError
    ? String(
        (transcriptQuery.error as { message?: string }).message ??
          transcriptQuery.error,
      )
    : choicesQuery.isError
      ? String(
          (choicesQuery.error as { message?: string }).message ??
            choicesQuery.error,
        )
      : selected && !selected.supported
        ? "已在画面显示位图字幕；文稿需文本轨或同名外挂 .srt"
        : null;

  return (
    <section className="flex max-h-64 flex-col border-t border-border">
      <div className="flex flex-wrap items-center gap-3 px-6 py-2 text-sm">
        <label className="flex items-center gap-2">
          <span className="font-medium">字幕</span>
          <select
            className="min-w-[16rem] rounded border border-border bg-background px-2 py-1"
            value={choiceId ?? ""}
            disabled={choices.length === 0}
            onChange={(e) => setChoiceId(e.target.value || null)}
            aria-label="Subtitle track"
          >
            {choices.length === 0 ? (
              <option value="">无可用字幕</option>
            ) : (
              choices.map((choice) => (
                <option key={choice.id} value={choice.id}>
                  {choice.label}
                </option>
              ))
            )}
          </select>
        </label>
        {choicesQuery.isLoading ? (
          <span className="text-muted-foreground">扫描字幕…</span>
        ) : null}
      </div>

      {transcriptQuery.isLoading && canLoadTranscript ? (
        <p className="px-6 pb-3 text-sm text-muted-foreground">加载文稿…</p>
      ) : null}

      {errorText && !transcript ? (
        <p className="px-6 pb-3 text-sm text-muted-foreground">{errorText}</p>
      ) : null}

      {!errorText && choices.length === 0 && !choicesQuery.isLoading ? (
        <p className="px-6 pb-3 text-sm text-muted-foreground">
          未找到内嵌字幕，也没有同名外挂（如 video.srt / video.en.srt）。
        </p>
      ) : null}

      {transcript ? (
        <ul className="min-h-0 flex-1 space-y-1 overflow-y-auto px-4 pb-3">
          {transcript.cues.map((cue, i) => {
            const active = i === activeIndex;
            return (
              <li key={cue.index} id={`cue-${i}`}>
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
