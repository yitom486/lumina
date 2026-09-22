import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
} from "@lumina/chat-ui/askAboutStore";
import { useChatUiStore } from "@lumina/chat-ui/chatUiStore";
import { resolveModelSelection } from "@lumina/chat-ui/modelConfig";
import type { AsrRange } from "@/features/asr";
import { useMediaInfoQuery } from "@/features/media";
import type { Transcript } from "@/features/transcript";
import { formatTime } from "@/lib/format";
import { useNoteComposeStore } from "@/features/notes/noteComposeStore";
import { usePlayerStore } from "@/features/player";
import { applySubtitleChoice } from "@/features/player/hooks/useTrackControls";
import { useUiStore } from "@/features/player/uiStore";
import { useTrackStore } from "@/features/player/trackStore";
import { getCachedYtdlResolve } from "@/features/ytdl";
import { Button } from "@lumina/ui/button";
import { ScrollArea } from "@lumina/ui/scroll-area";
import { cn } from "@lumina/ui/utils";

import {
  activeCueIndex,
  LANG_PRESETS,
  type AsrScope,
} from "../../../../../../packages/transcript-ui/src/cueSelectors";
import {
  listSubtitleChoices,
  loadSubtitleChoice,
  proofreadSubtitleTrack,
  translateSubtitleTrack,
  getWorkshopStatus,
  WORKSHOP_PROGRESS_EVENT,
  type WorkshopJobSnapshot,
} from "../api";
import { listen } from "@tauri-apps/api/event";
import { subtitleChoicesKey, transcriptKey, ytdlResolveKey } from "@lumina/query-keys";
import { OnlineSubtitleSection } from "./OnlineSubtitleSection";
import { useSubtitleWorkshopModels } from "../useSubtitleWorkshopModels";
import type { SubtitleChoice } from "@lumina/contracts";
import { followMode, useFollowStore, useSubtitleWorkshopStore } from "@lumina/transcript-ui";
import { findChapterAt } from "../../../../../../packages/transcript-ui/src/chapterSelectors";

// 翻译/校对任务超过此时长没有任何进度事件即判死：正常运行时每完成一批
// （4 并发）必推一次进度，10 分钟静默只可能发生在后端进程已死的情况下。
const TASK_STALL_MS = 10 * 60 * 1000;

// 工坊账本恢复文案（固定业务中文，只展示 message，永不展示 details）。
const WORKSHOP_FINISHED_TEXT = "已完成";
const WORKSHOP_FAILED_TEXT = "字幕任务失败，请重试";

function isProofreadSnapshot(snapshot: WorkshopJobSnapshot): boolean {
  return (
    snapshot.targetLang === "proofread" ||
    snapshot.targetLang.endsWith("-proofread")
  );
}

export type TranscriptPanelView = "reading" | "workshop";

export function TranscriptPanel({
  view = "reading",
}: {
  view?: TranscriptPanelView;
}) {
  const workshopOnly = view === "workshop";
  const queryClient = useQueryClient();
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const seek = usePlayerStore((s) => s.seek);
  const setSubtitle = usePlayerStore((s) => s.setSubtitle);

  const choiceId = useTrackStore((s) => s.subtitleChoiceId);
  const setSubtitleChoiceId = useTrackStore((s) => s.setSubtitleChoiceId);
  const setSubtitleVisible = useTrackStore((s) => s.setSubtitleVisible);
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
    queryKey: ytdlResolveKey(path),
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

  // Hooks must stay above the `!mediaReady` early return: media readiness
  // flips false -> true while opening a video, and any hook below the return
  // would change hook order across renders ("Rendered fewer hooks").
  const isRemotePath = /^https?:\/\//i.test(path ?? "");
  const audioLanguage = useMemo(
    () =>
      mediaInfoQuery.data?.streams?.find((stream) => stream.kind === "Audio")
        ?.language ?? null,
    [mediaInfoQuery.data],
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

  const [targetLang, setTargetLang] = useState("zh");
  const [listCollapsed, setListCollapsed] = useState(false);
  const backfillGlossary = useSubtitleWorkshopStore((s) => s.backfillGlossary);
  const glossaryReviewMode = useSubtitleWorkshopStore((s) => s.glossaryReviewMode);
  const patchWorkshopSettings = useSubtitleWorkshopStore((s) => s.patchSettings);
  const workshopSettings = { backfillGlossary, glossaryReviewMode };
  const [translateBusy, setTranslateBusy] = useState(false);
  const [translateProgress, setTranslateProgress] = useState<string | null>(
    null,
  );
  const [translateError, setTranslateError] = useState<string | null>(null);
  const [proofreadBusy, setProofreadBusy] = useState(false);
  const [proofreadProgress, setProofreadProgress] = useState<string | null>(
    null,
  );
  const [proofreadError, setProofreadError] = useState<string | null>(null);
  const [batchCount, setBatchCount] = useState<{
    done: number;
    total: number;
  } | null>(null);
  // 后台任务心跳：翻译/校对是长任务，进度靠后端单向推送；若后端进程
  // 被杀（dev 重编/退出应用），invoke 永不结算，UI 会 frozen 在最后一格。
  // 超过上限没有任何进展就判死并解锁按钮，不再无限等待。
  const taskHeartbeat = useRef<{
    task: "translate" | "proofread";
    lastAt: number;
  } | null>(null);
  // Channel 发起中的任务：后端会同时推 Channel + 全局广播，收到相同 job_id
  // 的广播时跳过（防 double 显示），Channel 路径已覆盖显示。
  const translateChannelActiveRef = useRef(false);
  const proofreadChannelActiveRef = useRef(false);
  const activeTranslateJobIdRef = useRef<string | null>(null);
  const activeProofreadJobIdRef = useRef<string | null>(null);

  useEffect(() => {
    const timer = setInterval(() => {
      const beat = taskHeartbeat.current;
      if (!beat || Date.now() - beat.lastAt <= TASK_STALL_MS) return;
      taskHeartbeat.current = null;
      if (beat.task === "translate") {
        setTranslateBusy(false);
        setTranslateProgress(null);
        setTranslateError(
          "字幕任务长时间没有进展，已停止等待；可直接重试",
        );
      } else {
        setProofreadBusy(false);
        setProofreadProgress(null);
        setProofreadError("字幕任务长时间没有进展，已停止等待；可直接重试");
      }
    }, 30_000);
    return () => clearInterval(timer);
  }, []);
  const stripSoundTags = useSubtitleWorkshopStore((s) => s.stripSoundTags);

  useEffect(() => {
    setAsrTranscript(null);
    setListCollapsed(false);
    setAsrProgress(null);
    setAsrError(null);
    setTranslateProgress(null);
    setTranslateError(null);
    setProofreadProgress(null);
    setProofreadError(null);
    setInstallProgress(null);
    setInstallError(null);
    setAsrScope("full");
    resetFollowForMedia();
  }, [path, resetFollowForMedia]);

  // 工坊进度在切换页面后回来能继续显示：后端记账本 + 全局广播，面板卸载
  // 不丢任务。挂载时先订阅全局事件再查账本（顺序不能反，否则有漏网卡死）。
  useEffect(() => {
    if (!mediaReady || !path) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const currentPath = path;

    const applySnapshot = (snapshot: WorkshopJobSnapshot) => {
      if (snapshot.mediaPath !== currentPath) return;
      const proofread = isProofreadSnapshot(snapshot);
      if (proofread) {
        if (proofreadChannelActiveRef.current) {
          if (!activeProofreadJobIdRef.current) {
            activeProofreadJobIdRef.current = snapshot.jobId;
          }
          if (snapshot.jobId === activeProofreadJobIdRef.current) {
            return;
          }
          return;
        }
        if (snapshot.phase === "Running") {
          setProofreadBusy(true);
          setProofreadError(null);
          setProofreadProgress(snapshot.message);
          setBatchCount(
            snapshot.done != null && snapshot.total != null
              ? { done: snapshot.done, total: snapshot.total }
              : null,
          );
          taskHeartbeat.current = {
            task: "proofread",
            lastAt: Date.now(),
          };
        } else if (snapshot.phase === "Finished") {
          if (taskHeartbeat.current?.task === "proofread") {
            taskHeartbeat.current = null;
          }
          setProofreadBusy(false);
          setProofreadError(null);
          setProofreadProgress((prev) =>
            prev?.startsWith("已写入") ? prev : WORKSHOP_FINISHED_TEXT,
          );
        } else {
          if (taskHeartbeat.current?.task === "proofread") {
            taskHeartbeat.current = null;
          }
          setProofreadBusy(false);
          setProofreadProgress(null);
          setProofreadError((prev) => prev ?? WORKSHOP_FAILED_TEXT);
        }
        return;
      }
      if (translateChannelActiveRef.current) {
        if (!activeTranslateJobIdRef.current) {
          activeTranslateJobIdRef.current = snapshot.jobId;
        }
        if (snapshot.jobId === activeTranslateJobIdRef.current) {
          return;
        }
        return;
      }
      if (snapshot.phase === "Running") {
        setTranslateBusy(true);
        setTranslateError(null);
        setTranslateProgress(snapshot.message);
        setBatchCount(
          snapshot.done != null && snapshot.total != null
            ? { done: snapshot.done, total: snapshot.total }
            : null,
        );
        taskHeartbeat.current = { task: "translate", lastAt: Date.now() };
      } else if (snapshot.phase === "Finished") {
        if (taskHeartbeat.current?.task === "translate") {
          taskHeartbeat.current = null;
        }
        setTranslateBusy(false);
        setTranslateError(null);
        setTranslateProgress((prev) =>
          prev?.startsWith("已写入") ? prev : WORKSHOP_FINISHED_TEXT,
        );
      } else {
        if (taskHeartbeat.current?.task === "translate") {
          taskHeartbeat.current = null;
        }
        setTranslateBusy(false);
        setTranslateProgress(null);
        setTranslateError((prev) => prev ?? WORKSHOP_FAILED_TEXT);
      }
    };

    const setup = async () => {
      try {
        unlisten = await listen<WorkshopJobSnapshot>(
          WORKSHOP_PROGRESS_EVENT,
          (event) => {
            if (cancelled) return;
            applySnapshot(event.payload);
          },
        );
      } catch {
        unlisten = undefined;
      }
      if (cancelled) return;
      try {
        const snapshot = await getWorkshopStatus(currentPath);
        if (cancelled || !snapshot) return;
        applySnapshot(snapshot);
      } catch {
        // 账本缺席不阻塞面板（后端旧版本或任务从未发起）。
      }
    };
    void setup();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [path, mediaReady]);

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
    // 新轨默认上屏（下载/翻译/校对/ASR 完成后沿用此前行为）。
    setSubtitleVisible(true);
    rememberSubtitleForMedia(mediaPath, exported);
    // Newly arrived tracks (downloaded, translated, ASR) show on the video
    // surface by default; the user can still switch tracks or turn subtitles
    // off, and the choice is remembered per media.
    await applySubtitleChoice(exported, setSubtitle, mediaPath, loadSubtitleChoice);
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

  async function handleProofread() {
    if (!path || !choiceId || proofreadBusy || asrBusy || translateBusy) return;
    if (!selected?.supported) {
      setProofreadError("请先选择可用的文本字幕轨");
      return;
    }
    proofreadChannelActiveRef.current = true;
    activeProofreadJobIdRef.current = null;
    setProofreadBusy(true);
    setProofreadError(null);
    setProofreadProgress("准备校对字幕…");
    setBatchCount(null);
    taskHeartbeat.current = { task: "proofread", lastAt: Date.now() };
    try {
      const result = await proofreadSubtitleTrack({
        path,
        choiceId,
        stripSoundTags,
        profileId: workshopModels.profileId,
        profiles: workshopModels.profilesHint,
        ...resolveModelSelection({
          modelId: workshopModels.modelId,
          reasoningEffort: workshopModels.reasoningEffort,
        }),
        onEvent: (event) => {
          if (event.type === "Progress") {
            setProofreadProgress(event.payload.message);
            taskHeartbeat.current = { task: "proofread", lastAt: Date.now() };
            setBatchCount(
              event.payload.done != null && event.payload.total != null
                ? { done: event.payload.done, total: event.payload.total }
                : null,
            );
          } else if (event.type === "Failed") {
            setProofreadError(event.payload.message);
          }
        },
      });
      const exported = await selectExportedTrack(
        path,
        result,
        ".proofread.srt",
      );
      setProofreadProgress(
        exported
          ? `已写入 ${exported.label}（字幕工坊模型：${workshopModels.modelLabel}）`
          : "校对完成，请在字幕轨中手动选择",
      );
    } catch (error) {
      const message =
        typeof error === "object" && error && "message" in error
          ? String((error as { message: string }).message)
          : String(error);
      setProofreadError(message);
      setProofreadProgress(null);
    } finally {
      proofreadChannelActiveRef.current = false;
      if (taskHeartbeat.current?.task === "proofread") {
        taskHeartbeat.current = null;
      }
      setProofreadBusy(false);
    }
  }

  async function handleTranslate() {
    if (!path || !choiceId || translateBusy || asrBusy) return;
    if (!selected?.supported) {
      setTranslateError("请先选择可用的文本字幕轨");
      return;
    }
    translateChannelActiveRef.current = true;
    activeTranslateJobIdRef.current = null;
    setTranslateBusy(true);
    setTranslateError(null);
    setTranslateProgress("准备翻译字幕…");
    setBatchCount(null);
    taskHeartbeat.current = { task: "translate", lastAt: Date.now() };
    try {
      const result = await translateSubtitleTrack({
        path,
        choiceId,
        targetLang,
        profileId: workshopModels.profileId,
        profiles: workshopModels.profilesHint,
        ...resolveModelSelection({
          modelId: workshopModels.modelId,
          reasoningEffort: workshopModels.reasoningEffort,
        }),
        glossaryBackfill: workshopSettings.backfillGlossary,
        glossaryReviewMode: workshopSettings.glossaryReviewMode,
        onEvent: (event) => {
          if (event.type === "Progress") {
            setTranslateProgress(event.payload.message);
            taskHeartbeat.current = { task: "translate", lastAt: Date.now() };
            setBatchCount(
              event.payload.done != null && event.payload.total != null
                ? { done: event.payload.done, total: event.payload.total }
                : null,
            );
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
      translateChannelActiveRef.current = false;
      if (taskHeartbeat.current?.task === "translate") {
        taskHeartbeat.current = null;
      }
      setTranslateBusy(false);
    }
  }

  if (!mediaReady && !workshopOnly) {
    return (
      <section className="flex min-h-0 flex-1 flex-col px-3 py-3 text-sm text-muted-foreground">
        <p className="mt-1 text-xs leading-relaxed">
          打开视频后，可在右侧切换音轨/字幕，并在此浏览文稿、按需 ASR 或翻译字幕。
        </p>
      </section>
    );
  }

  const choices = choicesQuery.data ?? [];

  async function handleDownloadedSubtitle(choiceId: string) {
    if (!path) return;
    await queryClient.invalidateQueries({
      queryKey: subtitleChoicesKey(path),
    });
    const fresh = await listSubtitleChoices(path);
    queryClient.setQueryData(subtitleChoicesKey(path), fresh);
    // Downloaded tracks show on the video surface by default and are
    // remembered; the user can still switch tracks or turn subtitles off.
    const downloaded = fresh.find((choice) => choice.id === choiceId);
    if (downloaded) {
      setAsrError(null);
      setTranslateError(null);
      setSubtitleChoiceId(choiceId);
      setSubtitleVisible(true);
      rememberSubtitleForMedia(path, downloaded);
      await applySubtitleChoice(downloaded, setSubtitle, path, loadSubtitleChoice);
      await queryClient.invalidateQueries({
        queryKey: transcriptKey(path, choiceId),
      });
    }
  }

  const asrAvailable = asrStatusQuery.data?.available === true;
  const installSupported = asrStatusQuery.data?.installSupported === true;
  const catalog = asrStatusQuery.data?.catalog ?? [];
  const canTranslate = Boolean(choiceId && selected?.supported);
  const busy = asrBusy || translateBusy || installBusy || proofreadBusy;
  const errorText = asrError
    ? asrError
    : installError
      ? installError
      : translateError
      ? translateError
      : proofreadError
        ? proofreadError
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
    <section className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <div className="flex shrink-0 flex-col gap-2 border-b border-border px-3 py-2">
        {!workshopOnly ? (
          <>
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
            <Button
              type="button"
              size="sm"
              variant="ghost"
              aria-expanded={!listCollapsed}
              aria-label={listCollapsed ? "展开文稿列表" : "收起文稿列表"}
              title={listCollapsed ? "展开文稿列表" : "收起文稿列表"}
              onClick={() => setListCollapsed((collapsed) => !collapsed)}
            >
              {listCollapsed ? "展开文稿" : "收起文稿"}
            </Button>
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
              const next = e.target.value || null;
              setSubtitleChoiceId(next);
              // 显式选轨即展示；隐藏只能走播放条的显示开关。
              if (next) setSubtitleVisible(true);
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

          </>
        ) : null}

        {workshopOnly ? (
          <>
        {mediaReady && !isRemotePath ? (
          <OnlineSubtitleSection
            mediaPath={path as string}
            audioLanguage={audioLanguage}
            disabled={busy}
            onDownloaded={(downloadedId) => void handleDownloadedSubtitle(downloadedId)}
          />
        ) : null}
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
          <label className="flex items-center gap-1 text-xs text-muted-foreground">
            <input
              type="checkbox"
              checked={backfillGlossary}
              disabled={busy}
              onChange={(e) => patchWorkshopSettings({ backfillGlossary: e.target.checked })}
            />
            回填译名表
          </label>
          <label className="flex items-center gap-1 text-xs text-muted-foreground">
            <input
              type="checkbox"
              checked={glossaryReviewMode}
              disabled={busy}
              onChange={(e) => patchWorkshopSettings({ glossaryReviewMode: e.target.checked })}
            />
            审核模式
          </label>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={busy || !canTranslate}
            onClick={() => void handleProofread()}
            title="校对源语言字幕（错别字/OCR/误听），不翻译不改时间轴"
          >
            {proofreadBusy ? "校对中…" : "校对字幕"}
          </Button>
          <label className="flex items-center gap-1 text-xs text-muted-foreground">
            <input
              type="checkbox"
              checked={stripSoundTags}
              disabled={busy}
              onChange={(e) => patchWorkshopSettings({ stripSoundTags: e.target.checked })}
            />
            去音效标签
          </label>
        </div>
        {proofreadProgress || proofreadError ? (
          <p className="text-[11px] leading-snug text-muted-foreground">
            {proofreadError ?? proofreadProgress}
          </p>
        ) : null}
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
        {translateError || proofreadError ? (
          <p className="text-xs text-destructive" role="alert">
            {translateError ?? proofreadError}
          </p>
        ) : null}
        {(translateBusy || proofreadBusy) &&
        batchCount &&
        batchCount.total > 0 ? (
          <div
            className="h-1 overflow-hidden rounded bg-border"
            role="progressbar"
            aria-valuenow={Math.min(batchCount.done, batchCount.total)}
            aria-valuemin={0}
            aria-valuemax={batchCount.total}
            aria-label="字幕批处理进度"
          >
            <div
              className="h-full bg-primary transition-all"
              style={{
                width: `${Math.min(100, (batchCount.done / batchCount.total) * 100)}%`,
              }}
            />
          </div>
        ) : null}
        {!asrAvailable && asrStatusQuery.data ? (
          <p className="text-[11px] leading-snug text-muted-foreground">
            {asrStatusQuery.data.message}
            {installSupported
              ? "。选择上方模型后点「一键下载」即可（无需手动找文件）。"
              : ""}
          </p>
        ) : null}
          </>
        ) : null}
      </div>

      {!workshopOnly && (listCollapsed ? (
        <button
          type="button"
          className="mx-2 mt-1 shrink-0 rounded-md border border-border/50 px-2 py-1.5 text-left text-xs text-muted-foreground"
          aria-expanded={false}
          aria-label="展开文稿列表"
          onClick={() => setListCollapsed(false)}
        >
          文稿已收起{transcript ? `（${transcript.cues.length} 句）` : ""} · 点击展开
        </button>
      ) : (
      <ScrollArea className="min-h-96 shrink-0 flex-1">
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
      ))}
    </section>
  );
}
