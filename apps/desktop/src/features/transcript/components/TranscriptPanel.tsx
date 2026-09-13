import { useCallback, useEffect, useMemo, useState } from "react";
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import { getAsrStatus, installAsrBundle, transcribeOnDemand } from "@/features/asr";
import {
  explainSegmentPreset,
  summarizeChapterPreset,
  useAskAboutStore,
} from "@/features/acp/askAboutStore";
import { useChatUiStore } from "@/features/acp/chatUiStore";
import type { AsrRange } from "@/features/asr";
import { useMediaInfoQuery } from "@/features/media";
import type { MediaChapter } from "@/features/media";
import type { Transcript } from "@/features/transcript";
import { formatTime } from "@/lib/format";
import { useNoteComposeStore } from "@/features/notes/noteComposeStore";
import { usePlayerStore } from "@/features/player";
import { applySubtitleChoice } from "@/features/player/hooks/useTrackControls";
import { useUiStore } from "@/features/player/uiStore";
import { useTrackStore } from "@/features/player/trackStore";
import { getCachedYtdlResolve } from "@/features/ytdl";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

import {
  listSubtitleChoices,
  loadSubtitleChoice,
  translateSubtitleTrack,
} from "../api";
import { subtitleChoicesKey, transcriptKey } from "../queries";
import { useSubtitleWorkshopModels } from "../useSubtitleWorkshopModels";
import type { Cue, SubtitleChoice } from "../types";
import { followMode, useFollowStore } from "../followStore";

function activeCueIndex(cues: Cue[], timeMs: number): number {
  return cues.findIndex((c) => timeMs >= c.startMs && timeMs < c.endMs);
}

function findChapterAt(
  chapters: MediaChapter[],
  timeMs: number,
): MediaChapter | null {
  if (chapters.length === 0) return null;
  const hit = chapters.find((chapter) => {
    if (timeMs < chapter.startMs) return false;
    if (chapter.endMs == null) return true;
    return timeMs < chapter.endMs;
  });
  return hit ?? null;
}

const LANG_PRESETS = [
  { value: "en", label: "英语 (en)" },
  { value: "zh", label: "中文 (zh)" },
  { value: "ja", label: "日语 (ja)" },
  { value: "ko", label: "韩语 (ko)" },
];

type AsrScope = "full" | "chapter";

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
  const setSidebarTab = useUiStore((s) => s.setSidebarTab);
  const noteQuoteIndices = useNoteComposeStore((s) => s.selectedIndices);
  const pickFromTranscript = useNoteComposeStore((s) => s.pickFromTranscript);
  const askAbout = useAskAboutStore((s) => s.askAbout);
  const openChat = useChatUiStore((s) => s.openChat);

  // P6-M1 controllable follow (independent from the prompt anchor).
  const followEnabled = useFollowStore((s) => s.followEnabled);
  const browsing = useFollowStore((s) => s.browsing);
  const setFollowEnabled = useFollowStore((s) => s.setFollowEnabled);
  const setBrowsing = useFollowStore((s) => s.setBrowsing);
  const resumeFollow = useFollowStore((s) => s.resume);
  const resetFollowForMedia = useFollowStore((s) => s.resetForMedia);
  const mode = followMode(followEnabled, browsing);

  const mediaReady =
    Boolean(path) &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  const mediaInfoQuery = useMediaInfoQuery();
  const onlineInfoQuery = useQuery({
    queryKey: ["ytdl-resolve", path],
    queryFn: () => getCachedYtdlResolve(path as string),
    enabled: mediaReady && /^https?:\/\//i.test(path ?? ""),
    staleTime: Infinity,
  });
  const chapters =
    mediaInfoQuery.data?.chapters ?? onlineInfoQuery.data?.chapters ?? [];
  const workshopModels = useSubtitleWorkshopModels(mediaReady);
  const activeChapter = useMemo(
    () => findChapterAt(chapters, currentTimeMs),
    [chapters, currentTimeMs],
  );

  const choicesQuery = useQuery({
    queryKey: subtitleChoicesKey(path),
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

  const [asrScope, setAsrScope] = useState<AsrScope>("full");
  const [asrModelId, setAsrModelId] = useState<string>("");
  const [installModelId, setInstallModelId] = useState("base");
  const [installBusy, setInstallBusy] = useState(false);
  const [installProgress, setInstallProgress] = useState<string | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
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
    setInstallProgress(null);
    setInstallError(null);
    setAsrScope("full");
    resetFollowForMedia();
  }, [path, resetFollowForMedia]);

  useEffect(() => {
    if (chapters.length === 0 && asrScope === "chapter") {
      setAsrScope("full");
    }
  }, [chapters.length, asrScope]);

  const asrModels = asrStatusQuery.data?.models ?? [];
  useEffect(() => {
    if (asrModels.length === 0) {
      setAsrModelId("");
      return;
    }
    if (asrModelId && asrModels.some((m) => m.id === asrModelId)) {
      return;
    }
    const preferred =
      asrModels.find((m) => /base|tiny|small/i.test(m.id)) ?? asrModels[0];
    setAsrModelId(preferred.id);
  }, [asrModels, asrModelId]);

  useEffect(() => {
    setAsrTranscript(null);
  }, [choiceId]);

  const selected = choicesQuery.data?.find((c) => c.id === choiceId);

  const canLoadTranscript = Boolean(
    choiceId && selected?.supported && !asrTranscript,
  );

  const transcriptQuery = useQuery({
    queryKey: transcriptKey(path, choiceId),
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

  const scrollToActive = useCallback(() => {
    if (activeIndex < 0 || showStale) return;
    const el = document.getElementById(`cue-${activeIndex}`);
    el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [activeIndex, showStale]);

  const markBrowsingOnSelection = useCallback(() => {
    const selection = window.getSelection();
    if (selection && !selection.isCollapsed) setBrowsing(true);
  }, [setBrowsing]);

  useEffect(() => {
    // Browsing or follow-off: never yank the page. Wheel/touch/keys drive
    // `browsing`, so programmatic scrolls need no suppression window.
    if (mode !== "following") return;
    scrollToActive();
  }, [mode, scrollToActive]);

  async function selectExportedTrack(
    mediaPath: string,
    result: Transcript,
    suffixHint?: string,
  ) {
    await queryClient.invalidateQueries({
      queryKey: subtitleChoicesKey(mediaPath),
    });
    const choices = await listSubtitleChoices(mediaPath);
    queryClient.setQueryData(subtitleChoicesKey(mediaPath), choices);
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
      queryKey: transcriptKey(mediaPath, exported.id),
    });
    setAsrTranscript(null);
    return exported;
  }

  async function handleInstallAsr() {
    if (installBusy || asrBusy || translateBusy) return;
    setInstallBusy(true);
    setInstallError(null);
    setInstallProgress("准备下载转写组件…");
    try {
      await installAsrBundle(installModelId, (event) => {
        if (event.type === "Progress") {
          setInstallProgress(event.payload.message);
        } else if (event.type === "Failed") {
          setInstallError(event.payload.message);
        }
      });
      await queryClient.invalidateQueries({ queryKey: ["asrStatus"] });
      setInstallProgress("转写组件已就绪，可生成字幕");
    } catch (error) {
      const message =
        typeof error === "object" && error && "message" in error
          ? String((error as { message: string }).message)
          : String(error);
      setInstallError(message);
      setInstallProgress(null);
    } finally {
      setInstallBusy(false);
    }
  }

  async function handleAsr() {
    if (!path || asrBusy || translateBusy) return;

    let range: AsrRange | null = null;
    let suffixHint = ".asr.srt";
    if (asrScope === "chapter") {
      if (!activeChapter) {
        setAsrError("当前位置不在任何章节内");
        return;
      }
      range = { kind: "chapter", chapterId: activeChapter.id };
      suffixHint = `.asr_ch${activeChapter.id}.srt`;
    }

    setAsrBusy(true);
    setAsrError(null);
    setAsrProgress("准备按需转写…");
    try {
      const result = await transcribeOnDemand(
        path,
        (event) => {
          if (event.type === "Progress") {
            setAsrProgress(event.payload.message);
          } else if (event.type === "Started") {
            setAsrProgress("已开始语音转写…");
          } else if (event.type === "Failed") {
            setAsrError(event.payload.message);
          }
        },
        {
          range,
          modelId: asrModelId || null,
        },
      );

      const exported = await selectExportedTrack(path, result, suffixHint);
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
    setTranslateProgress("准备翻译字幕…");
    try {
      const result = await translateSubtitleTrack({
        path,
        choiceId,
        targetLang,
        profileId: workshopModels.profileId,
        profiles: workshopModels.profilesHint,
        modelId: workshopModels.modelId || null,
        reasoningEffort: workshopModels.reasoningEffort || null,
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
          ? `已写入 ${exported.label}（字幕工坊模型：${workshopModels.modelLabel}）`
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
  const installSupported = asrStatusQuery.data?.installSupported === true;
  const catalog = asrStatusQuery.data?.catalog ?? [];
  const canTranslate = Boolean(choiceId && selected?.supported);
  const busy = asrBusy || translateBusy || installBusy;
  const errorText = asrError
    ? asrError
    : installError
      ? installError
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
          <div className="flex items-center gap-1">
            {mode === "browsing" ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={resumeFollow}
              >
                回到当前播放位置
              </Button>
            ) : null}
            <Button
              type="button"
              size="sm"
              variant={followEnabled ? "secondary" : "ghost"}
              aria-pressed={followEnabled}
              title={followEnabled ? "跟随播放位置（开）" : "跟随播放位置（关）"}
              onClick={() => setFollowEnabled(!followEnabled)}
            >
              跟随
            </Button>
            {transcriptQuery.isFetching || choicesQuery.isFetching ? (
              <span className="text-xs text-muted-foreground">加载中…</span>
            ) : null}
          </div>
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

        <div className="flex flex-wrap items-end gap-2">
          <label className="flex min-w-[7rem] flex-1 flex-col gap-1 text-xs">
            <span className="text-muted-foreground">ASR 范围</span>
            <select
              className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
              value={asrScope}
              disabled={busy}
              onChange={(e) => setAsrScope(e.target.value as AsrScope)}
              aria-label="ASR transcription scope"
            >
              <option value="full">整片</option>
              <option value="chapter" disabled={chapters.length === 0}>
                当前章节
                {chapters.length === 0 ? "（无章节）" : ""}
              </option>
            </select>
          </label>
          {asrModels.length > 0 ? (
            <label className="flex min-w-[8rem] flex-1 flex-col gap-1 text-xs">
              <span className="text-muted-foreground">转写模型</span>
              <select
                className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
                value={asrModelId}
                disabled={busy}
                onChange={(e) => setAsrModelId(e.target.value)}
                aria-label="ASR model"
              >
                {asrModels.map((model) => (
                  <option key={model.id} value={model.id}>
                    {model.id}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={
              busy ||
              !asrAvailable ||
              (asrScope === "chapter" && !activeChapter)
            }
            onClick={() => void handleAsr()}
            title={
              asrAvailable
                ? "按需转写并保存为同目录外挂字幕"
                : installSupported
                  ? "请先一键下载转写组件"
                  : "未配置转写组件"
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
        {asrScope === "chapter" ? (
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-[11px] text-muted-foreground">
              {activeChapter
                ? `当前章节：${activeChapter.title?.trim() || `第 ${activeChapter.id} 章`}（${formatTime(activeChapter.startMs)}${
                    activeChapter.endMs != null
                      ? `–${formatTime(activeChapter.endMs)}`
                      : "–片尾"
                  }）`
                : "请先 seek 到某一章节内"}
            </p>
            {activeChapter ? (
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="h-6 px-2 text-[11px]"
                onClick={() => {
                  const title =
                    activeChapter.title?.trim() ||
                    `第 ${activeChapter.id} 章`;
                  const range = `${formatTime(activeChapter.startMs)}–${
                    activeChapter.endMs != null
                      ? formatTime(activeChapter.endMs)
                      : "片尾"
                  }`;
                  askAbout(
                    activeChapter.startMs,
                    summarizeChapterPreset(title, range),
                  );
                  openChat();
                }}
              >
                总结本章
              </Button>
            ) : null}
          </div>
        ) : null}

        {installSupported ? (
          <div className="flex flex-wrap items-end gap-2 rounded-md border border-border/70 bg-muted/20 p-2">
            <label className="flex min-w-[9rem] flex-1 flex-col gap-1 text-xs">
              <span className="text-muted-foreground">
                {asrAvailable ? "下载更多模型" : "一键安装转写"}
              </span>
              <select
                className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
                value={installModelId}
                disabled={busy}
                onChange={(e) => setInstallModelId(e.target.value)}
                aria-label="ASR model to download"
              >
                {(catalog.length > 0
                  ? catalog
                  : [
                      {
                        id: "tiny",
                        label: "Tiny（快 · 约 75 MB）",
                        installed: false,
                      },
                      {
                        id: "base",
                        label: "Base（推荐 · 约 142 MB）",
                        installed: false,
                      },
                      {
                        id: "small",
                        label: "Small（更准 · 约 466 MB）",
                        installed: false,
                      },
                    ]
                ).map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                    {"installed" in item && item.installed ? " · 已安装" : ""}
                  </option>
                ))}
              </select>
            </label>
            <Button
              type="button"
              variant={asrAvailable ? "outline" : "default"}
              size="sm"
              disabled={busy}
              onClick={() => void handleInstallAsr()}
              title="下载转写引擎（若缺）与所选模型到本机应用数据目录"
            >
              {installBusy
                ? "下载中…"
                : asrAvailable
                  ? "下载模型"
                  : "一键下载"}
            </Button>
          </div>
        ) : null}

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
          {workshopModels.hasModelOptions ? (
            <label className="flex min-w-[8rem] flex-1 flex-col gap-1 text-xs">
              <span className="text-muted-foreground">翻译模型</span>
              <select
                className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
                value={workshopModels.modelId}
                disabled={busy}
                onChange={(e) =>
                  workshopModels.patchSettings({ modelId: e.target.value })
                }
                aria-label="Subtitle translation model"
              >
                <option value="">默认</option>
                {workshopModels.modelOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.name}
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <label className="flex min-w-[8rem] flex-1 flex-col gap-1 text-xs">
              <span className="text-muted-foreground">翻译模型 ID</span>
              <input
                className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
                value={workshopModels.modelId}
                disabled={busy}
                placeholder="如 gpt-5.6-luna"
                aria-label="Subtitle translation model id"
                onChange={(e) =>
                  workshopModels.patchSettings({ modelId: e.target.value })
                }
              />
            </label>
          )}
          {workshopModels.hasReasoningOptions ? (
            <label className="flex min-w-[6rem] flex-1 flex-col gap-1 text-xs">
              <span className="text-muted-foreground">思考程度</span>
              <select
                className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm disabled:opacity-50"
                value={workshopModels.reasoningEffort}
                disabled={busy}
                onChange={(e) =>
                  workshopModels.patchSettings({
                    reasoningEffort: e.target.value,
                  })
                }
                aria-label="Subtitle translation reasoning effort"
              >
                <option value="">默认</option>
                {workshopModels.reasoningOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.name}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={busy || !canTranslate}
            onClick={() => void handleTranslate()}
            title="使用字幕工坊独立会话翻译（不与 AI 对话共用）"
          >
            {translateBusy ? "翻译中…" : "翻译字幕"}
          </Button>
        </div>
        <p className="text-[11px] leading-snug text-muted-foreground">
          翻译走字幕工坊专用模型与隔离会话，不会写入 AI 对话历史；主聊天 Agent
          也不能制作/写入外挂字幕。
          {workshopModels.modelId.trim()
            ? ` 当前：${workshopModels.modelLabel}`
            : " 当前：默认模型"}
        </p>

        {installProgress ? (
          <p className="text-xs text-muted-foreground">{installProgress}</p>
        ) : null}
        {asrProgress ? (
          <p className="text-xs text-muted-foreground">{asrProgress}</p>
        ) : null}
        {translateProgress ? (
          <p className="text-xs text-muted-foreground">{translateProgress}</p>
        ) : null}
        {!asrAvailable && asrStatusQuery.data ? (
          <p className="text-[11px] leading-snug text-muted-foreground">
            {asrStatusQuery.data.message}
            {installSupported
              ? "。选择上方模型后点「一键下载」即可（无需手动找文件）。"
              : ""}
          </p>
        ) : null}
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div
          className={`px-2 py-2 ${showStale ? "opacity-50" : ""}`}
          onWheel={() => setBrowsing(true)}
          onTouchMove={() => setBrowsing(true)}
          onKeyDown={(event) => {
            if (event.shiftKey) {
              setBrowsing(true);
              return;
            }
            if (
              [
                "ArrowUp",
                "ArrowDown",
                "PageUp",
                "PageDown",
                "Home",
                "End",
                " ",
              ].includes(event.key)
            ) {
              setBrowsing(true);
            }
          }}
          onMouseUp={markBrowsingOnSelection}
          onTouchEnd={markBrowsingOnSelection}
        >
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
                const quoted = noteQuoteIndices.includes(cue.index);
                return (
                  <li
                    key={`${transcript.choiceId}-${cue.index}`}
                    id={`cue-${i}`}
                    className="group flex items-stretch gap-1"
                  >
                    <button
                      type="button"
                      className={cn(
                        "min-w-0 flex-1 rounded-md px-2 py-1.5 text-left text-sm transition-colors",
                        active
                          ? "bg-accent font-medium text-accent-foreground"
                          : "text-muted-foreground hover:bg-muted hover:text-foreground",
                        quoted && !active && "ring-1 ring-primary/30",
                      )}
                      onClick={() => {
                        // Jumping to a sentence means watching from there.
                        resumeFollow();
                        void seek(cue.startMs);
                      }}
                    >
                      <span className="mr-2 tabular-nums text-[11px] opacity-70">
                        {formatTime(cue.startMs)}
                      </span>
                      {cue.text}
                    </button>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      className="h-auto shrink-0 px-2 py-1 text-[10px] opacity-70 group-hover:opacity-100"
                      title={`就这句提问（锚定 ${formatTime(cue.startMs)}）`}
                      disabled={!path}
                      onClick={(event) => {
                        event.stopPropagation();
                        if (!path) return;
                        askAbout(
                          cue.startMs,
                          explainSegmentPreset(
                            cue.text,
                            formatTime(cue.startMs),
                          ),
                        );
                        openChat();
                      }}
                    >
                      问
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      variant={quoted ? "secondary" : "ghost"}
                      className="h-auto shrink-0 px-2 py-1 text-[10px] opacity-70 group-hover:opacity-100"
                      title="加入批注引用（Shift 连选一段）"
                      disabled={!path || !choiceId}
                      onClick={(event) => {
                        event.stopPropagation();
                        if (!path || !choiceId) return;
                        pickFromTranscript({
                          mediaPath: path,
                          subtitleChoiceId: choiceId,
                          cues: transcript.cues,
                          listIndex: i,
                          shiftKey: event.shiftKey,
                        });
                        setSidebarTab("notes");
                      }}
                    >
                      引用
                    </Button>
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
