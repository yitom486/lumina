import { useEffect, useMemo, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";

import { getAsrStatus, transcribeOnDemand } from "@/features/asr";
import type { Transcript } from "@/features/transcript";
import { formatTime } from "@/lib/format";
import { usePlayerStore } from "@/features/player";
import { useTrackStore } from "@/features/player/trackStore";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";

import { listSubtitleChoices, loadSubtitleChoice } from "../api";
import type { Cue } from "../types";

function activeCueIndex(cues: Cue[], timeMs: number): number {
  return cues.findIndex((c) => timeMs >= c.startMs && timeMs < c.endMs);
}

export function TranscriptPanel() {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const seek = usePlayerStore((s) => s.seek);

  const choiceId = useTrackStore((s) => s.subtitleChoiceId);
  const setSubtitleChoiceId = useTrackStore((s) => s.setSubtitleChoiceId);

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

  const asrStatusQuery = useQuery({
    queryKey: ["asrStatus"],
    queryFn: getAsrStatus,
    staleTime: 60_000,
    retry: false,
  });

  const [asrTranscript, setAsrTranscript] = useState<Transcript | null>(null);
  const [asrBusy, setAsrBusy] = useState(false);
  const [asrProgress, setAsrProgress] = useState<string | null>(null);
  const [asrError, setAsrError] = useState<string | null>(null);

  useEffect(() => {
    setAsrTranscript(null);
    setAsrProgress(null);
    setAsrError(null);
  }, [path]);

  useEffect(() => {
    setAsrTranscript(null);
  }, [choiceId]);

  const selected = choicesQuery.data?.find((c) => c.id === choiceId);

  const canLoadTranscript = Boolean(
    choiceId && selected?.supported && !asrTranscript,
  );

  const transcriptQuery = useQuery({
    queryKey: ["transcript", path, choiceId],
    queryFn: () => loadSubtitleChoice(path as string, choiceId as string),
    enabled: mediaReady && canLoadTranscript,
    retry: false,
    placeholderData: keepPreviousData,
  });

  const transcript = asrTranscript ?? transcriptQuery.data ?? null;
  const showStale =
    !asrTranscript &&
    transcriptQuery.isFetching &&
    transcript != null &&
    transcript.choiceId !== choiceId;

  const activeIndex = useMemo(
    () => (transcript ? activeCueIndex(transcript.cues, currentTimeMs) : -1),
    [transcript, currentTimeMs],
  );

  useEffect(() => {
    if (activeIndex < 0 || showStale) return;
    const el = document.getElementById(`cue-${activeIndex}`);
    el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [activeIndex, showStale]);

  async function handleAsr() {
    if (!path || asrBusy) return;
    setAsrBusy(true);
    setAsrError(null);
    setAsrProgress("准备按需 ASR…");
    try {
      const result = await transcribeOnDemand(path, (event) => {
        if (event.type === "Progress") {
          setAsrProgress(event.payload.message);
        } else if (event.type === "Started") {
          setAsrProgress("已开始（首次才会启动 whisper-cli）…");
        } else if (event.type === "Failed") {
          setAsrError(event.payload.message);
        }
      });
      setAsrTranscript(result);
      setAsrProgress(null);
    } catch (error) {
      const message =
        typeof error === "object" && error && "message" in error
          ? String((error as { message: string }).message)
          : String(error);
      setAsrError(message);
      setAsrProgress(null);
    } finally {
      setAsrBusy(false);
    }
  }

  if (!mediaReady) {
    return (
      <section className="flex min-h-0 flex-1 flex-col px-3 py-3 text-sm text-muted-foreground">
        <p className="mt-1 text-xs leading-relaxed">
          打开视频后，可在右侧切换音轨/字幕，并在此浏览文稿或按需 ASR。
        </p>
      </section>
    );
  }

  const choices = choicesQuery.data ?? [];
  const asrAvailable = asrStatusQuery.data?.available === true;
  const errorText = asrError
    ? asrError
    : transcriptQuery.isError && !asrTranscript && !transcriptQuery.isFetching
      ? String(
          (transcriptQuery.error as { message?: string }).message ??
            transcriptQuery.error,
        )
      : choicesQuery.isError
        ? String(
            (choicesQuery.error as { message?: string }).message ??
              choicesQuery.error,
          )
        : selected && !selected.supported && !asrTranscript
          ? "已在画面显示位图字幕；文稿可点 ASR，或在播放条换文本轨"
          : null;

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 flex-col gap-2 border-b border-border px-3 py-2">
        <div className="flex items-center justify-between gap-2">
          <p className="text-sm font-medium">文稿</p>
          {transcriptQuery.isFetching || choicesQuery.isFetching ? (
            <span className="text-xs text-muted-foreground">加载中…</span>
          ) : null}
        </div>

        <p className="text-[11px] text-muted-foreground">
          音轨/字幕请用右侧边栏切换
          {selected ? ` · 当前：${selected.label}` : ""}
        </p>

        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">文稿对应字幕轨</span>
          <select
            className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
            value={choiceId ?? ""}
            disabled={choices.length === 0}
            onChange={(e) => {
              setAsrError(null);
              setSubtitleChoiceId(e.target.value || null);
            }}
            aria-label="Subtitle track for transcript"
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

        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={asrBusy}
            onClick={() => void handleAsr()}
            title={
              asrAvailable
                ? "按需启动本地 whisper-cli（不预加载）"
                : "未配置 ASR 也可点，会提示如何放置"
            }
          >
            {asrBusy ? "生成中…" : "生成文稿 (ASR)"}
          </Button>
          {asrTranscript ? (
            <button
              type="button"
              className="text-xs text-muted-foreground underline"
              onClick={() => setAsrTranscript(null)}
            >
              回到字幕文稿
            </button>
          ) : null}
        </div>

        {asrProgress ? (
          <p className="text-xs text-muted-foreground">{asrProgress}</p>
        ) : null}
        {!asrAvailable && asrStatusQuery.data ? (
          <p className="text-[11px] leading-snug text-muted-foreground">
            ASR 未配置：{asrStatusQuery.data.message}
          </p>
        ) : null}
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className={`px-2 py-2 ${showStale ? "opacity-50" : ""}`}>
          {transcriptQuery.isFetching && !transcript ? (
            <p className="px-2 text-sm text-muted-foreground">加载文稿…</p>
          ) : null}

          {errorText && !transcript ? (
            <p className="px-2 text-sm text-muted-foreground">{errorText}</p>
          ) : null}

          {!errorText &&
          choices.length === 0 &&
          !choicesQuery.isLoading &&
          !asrTranscript ? (
            <p className="px-2 text-sm text-muted-foreground">
              无文本字幕时，可按需使用 ASR（需本地 whisper-cli）。
            </p>
          ) : null}

          {transcript ? (
            <ul className="space-y-0.5">
              {transcript.cues.map((cue, i) => {
                const active = !showStale && i === activeIndex;
                return (
                  <li key={`${transcript.choiceId}-${cue.index}`} id={`cue-${i}`}>
                    <button
                      type="button"
                      className={`w-full rounded-md px-2 py-1.5 text-left text-sm transition-colors ${
                        active
                          ? "bg-accent font-medium text-accent-foreground"
                          : "text-muted-foreground hover:bg-muted hover:text-foreground"
                      }`}
                      onClick={() => void seek(cue.startMs)}
                    >
                      <span className="mr-2 tabular-nums text-[11px] opacity-70">
                        {formatTime(cue.startMs)}
                      </span>
                      {cue.text}
                    </button>
                  </li>
                );
              })}
            </ul>
          ) : null}
        </div>
      </ScrollArea>
    </section>
  );
}
