import { useEffect, useMemo, useState } from "react";
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import { getAsrStatus, transcribeOnDemand } from "@/features/asr";
import { useAcpProfilesStore } from "@/features/acp/acpProfilesStore";
import { useAcpSettingsStore } from "@/features/acp/acpSettingsStore";
import { profilesHintFromStore } from "@/features/acp/defaultAgentProfiles";
import type { Transcript } from "@/features/transcript";
import { formatTime } from "@/lib/format";
import { usePlayerStore } from "@/features/player";
import { applySubtitleChoice } from "@/features/player/hooks/useTrackControls";
import { useTrackStore } from "@/features/player/trackStore";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";

import {
  listSubtitleChoices,
  loadSubtitleChoice,
  translateSubtitleTrack,
} from "../api";
import type { Cue, SubtitleChoice } from "../types";

function activeCueIndex(cues: Cue[], timeMs: number): number {
  return cues.findIndex((c) => timeMs >= c.startMs && timeMs < c.endMs);
}

const LANG_PRESETS = [
  { value: "en", label: "英语 (en)" },
  { value: "zh", label: "中文 (zh)" },
  { value: "ja", label: "日语 (ja)" },
  { value: "ko", label: "韩语 (ko)" },
];

export function TranscriptPanel() {
  const queryClient = useQueryClient();
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const seek = usePlayerStore((s) => s.seek);
  const setSubtitle = usePlayerStore((s) => s.setSubtitle);

  const choiceId = useTrackStore((s) => s.subtitleChoiceId);
  const setSubtitleChoiceId = useTrackStore((s) => s.setSubtitleChoiceId);
  const rememberSubtitleForMedia = useTrackStore(
    (s) => s.rememberSubtitleForMedia,
  );

  const activeProfileId = useAcpProfilesStore((s) => s.activeProfileId);
  const profiles = useAcpProfilesStore((s) => s.profiles);
  const modelId = useAcpSettingsStore((s) => s.modelId);
  const reasoningEffort = useAcpSettingsStore((s) => s.reasoningEffort);

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
    staleTime: Infinity,
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

  const [targetLang, setTargetLang] = useState("en");
  const [translateBusy, setTranslateBusy] = useState(false);
  const [translateProgress, setTranslateProgress] = useState<string | null>(
    null,
  );
  const [translateError, setTranslateError] = useState<string | null>(null);

  useEffect(() => {
    setAsrTranscript(null);
    setAsrProgress(null);
    setAsrError(null);
    setTranslateProgress(null);
    setTranslateError(null);
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
    staleTime: Infinity,
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

  async function selectExportedTrack(
    mediaPath: string,
    result: Transcript,
    suffixHint?: string,
  ) {
    await queryClient.invalidateQueries({
      queryKey: ["subtitleChoices", mediaPath],
    });
    const choices = await listSubtitleChoices(mediaPath);
    queryClient.setQueryData(["subtitleChoices", mediaPath], choices);
    const exported =
      choices.find((choice) => choice.id === result.choiceId) ??
      (suffixHint
        ? choices.find((choice) =>
            choice.externalPath?.toLowerCase().endsWith(suffixHint),
          )
        : undefined);

    if (!exported) {
      setAsrTranscript(result);
      return null;
    }

    setSubtitleChoiceId(exported.id);
    rememberSubtitleForMedia(mediaPath, exported);
    await applySubtitleChoice(exported, setSubtitle);
    await queryClient.invalidateQueries({
      queryKey: ["transcript", mediaPath, exported.id],
    });
    setAsrTranscript(null);
    return exported;
  }

  async function handleAsr() {
    if (!path || asrBusy || translateBusy) return;
    setAsrBusy(true);
    setAsrError(null);
    setAsrProgress("准备按需转写…");
    try {
      const result = await transcribeOnDemand(path, (event) => {
        if (event.type === "Progress") {
          setAsrProgress(event.payload.message);
        } else if (event.type === "Started") {
          setAsrProgress("已开始语音转写…");
        } else if (event.type === "Failed") {
          setAsrError(event.payload.message);
        }
      });

      const exported = await selectExportedTrack(path, result, ".asr.srt");
      setAsrProgress(
        exported ? "已保存外挂字幕并切换到该轨" : null,
      );
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

  async function handleTranslate() {
    if (!path || !choiceId || translateBusy || asrBusy) return;
    if (!selected?.supported) {
      setTranslateError("请先选择可用的文本字幕轨");
      return;
    }
    setTranslateBusy(true);
    setTranslateError(null);
    setTranslateProgress("准备用 Agent 翻译…");
    try {
      const profilesHint = profilesHintFromStore(activeProfileId, profiles);
      const result = await translateSubtitleTrack({
        path,
        choiceId,
        targetLang,
        profileId: activeProfileId,
        profiles: profilesHint,
        modelId: modelId || null,
        reasoningEffort: reasoningEffort || null,
        onEvent: (event) => {
          if (event.type === "Progress") {
            setTranslateProgress(event.payload.message);
          } else if (event.type === "Failed") {
            setTranslateError(event.payload.message);
          }
        },
      });
      const exported = await selectExportedTrack(
        path,
        result,
        `.${targetLang.toLowerCase()}.srt`,
      );
      setTranslateProgress(
        exported
          ? `已写入 ${exported.label}（可用 Agent 设置中的模型）`
          : "翻译完成，请在字幕轨中手动选择",
      );
    } catch (error) {
      const message =
        typeof error === "object" && error && "message" in error
          ? String((error as { message: string }).message)
          : String(error);
      setTranslateError(message);
      setTranslateProgress(null);
    } finally {
      setTranslateBusy(false);
    }
  }

  if (!mediaReady) {
    return (
      <section className="flex min-h-0 flex-1 flex-col px-3 py-3 text-sm text-muted-foreground">
        <p className="mt-1 text-xs leading-relaxed">
          打开视频后，可在右侧切换音轨/字幕，并在此浏览文稿、按需 ASR 或翻译字幕。
        </p>
      </section>
    );
  }

  const choices = choicesQuery.data ?? [];
  const asrAvailable = asrStatusQuery.data?.available === true;
  const canTranslate = Boolean(choiceId && selected?.supported);
  const busy = asrBusy || translateBusy;
  const errorText = asrError
    ? asrError
    : translateError
      ? translateError
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
            disabled={choices.length === 0 || busy}
            onChange={(e) => {
              setAsrError(null);
              setTranslateError(null);
              setSubtitleChoiceId(e.target.value || null);
            }}
            aria-label="Subtitle track for transcript"
          >
            {choices.length === 0 ? (
              <option value="">无可用字幕</option>
            ) : (
              choices.map((choice: SubtitleChoice) => (
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
            disabled={busy}
            onClick={() => void handleAsr()}
            title={
              asrAvailable
                ? "按需转写并保存为同目录外挂字幕"
                : "未配置转写组件也可点，会提示如何放置"
            }
          >
            {asrBusy ? "生成中…" : "生成字幕 (ASR)"}
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

        <div className="flex flex-wrap items-end gap-2 rounded-md border border-border/70 bg-muted/20 p-2">
          <label className="flex min-w-[7rem] flex-1 flex-col gap-1 text-xs">
            <span className="text-muted-foreground">翻译目标语言</span>
            <select
              className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
              value={targetLang}
              disabled={busy}
              onChange={(e) => setTargetLang(e.target.value)}
              aria-label="Target language for subtitle translation"
            >
              {LANG_PRESETS.map((lang) => (
                <option key={lang.value} value={lang.value}>
                  {lang.label}
                </option>
              ))}
            </select>
          </label>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={busy || !canTranslate}
            onClick={() => void handleTranslate()}
            title="使用当前 Agent 配置中的模型翻译并写成新外挂轨（可选）"
          >
            {translateBusy ? "翻译中…" : "翻译字幕"}
          </Button>
        </div>
        <p className="text-[11px] leading-snug text-muted-foreground">
          翻译使用 Agent 设置里的模型（如 GPT Luna）；也可在 AI 对话里让 Agent
          用工具一句/一批翻译后回填。
          {modelId ? ` 当前模型：${modelId}` : " 当前：Agent 默认模型"}
        </p>

        {asrProgress ? (
          <p className="text-xs text-muted-foreground">{asrProgress}</p>
        ) : null}
        {translateProgress ? (
          <p className="text-xs text-muted-foreground">{translateProgress}</p>
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
              无文本字幕时，可按需使用 ASR 生成外挂字幕。
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
