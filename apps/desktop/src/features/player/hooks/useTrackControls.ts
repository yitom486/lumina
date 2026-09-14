import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";

import { useMediaInfoQuery } from "@/features/media";
import {
  listSubtitleChoices,
  loadSubtitleChoice,
} from "@/features/transcript/api";
import { subtitleChoicesKey } from "@lumina/query-keys";

import { usePlayerStore } from "../store";
import {
  mediaDirectoryKey,
  resolveDirectoryTrackSelection,
  pickDefaultSubtitleId,
} from "@lumina/player-ui";
import { useTrackStore } from "../trackStore";

/** Compat facade: canonical implementations live in @lumina/player-ui. */
import {
  applySubtitleChoice,
  audioTrackLabel,
} from "@lumina/player-ui/trackActions";
export { applySubtitleChoice, audioTrackLabel };

export function useTrackControls() {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const setSubtitle = usePlayerStore((s) => s.setSubtitle);
  const setAudio = usePlayerStore((s) => s.setAudio);

  const subtitleChoiceId = useTrackStore((s) => s.subtitleChoiceId);
  const setSubtitleChoiceId = useTrackStore((s) => s.setSubtitleChoiceId);
  const subtitleVisible = useTrackStore((s) => s.subtitleVisible);
  const setSubtitleVisible = useTrackStore((s) => s.setSubtitleVisible);
  const audioStreamIndex = useTrackStore((s) => s.audioStreamIndex);
  const setAudioStreamIndex = useTrackStore((s) => s.setAudioStreamIndex);
  const rememberSubtitleForMedia = useTrackStore(
    (s) => s.rememberSubtitleForMedia,
  );
  const rememberAudioForMedia = useTrackStore((s) => s.rememberAudioForMedia);
  const directoryPrefs = useTrackStore((s) => s.directoryPrefs);

  const initRef = useRef<string | null>(null);

  const mediaReady =
    Boolean(path) &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  const choicesQuery = useQuery({
    queryKey: subtitleChoicesKey(path),
    queryFn: () => listSubtitleChoices(path as string),
    enabled: mediaReady,
    retry: false,
    staleTime: Infinity,
  });

  const mediaQuery = useMediaInfoQuery();
  const audioTracks =
    mediaQuery.data?.streams.filter((s) => s.kind === "Audio") ?? [];

  const choices = choicesQuery.data ?? [];

  useEffect(() => {
    initRef.current = null;
  }, [path]);

  useEffect(() => {
    if (!mediaReady || !path) return;
    if (!choicesQuery.isFetched && !mediaQuery.isFetched) return;

    const dirKey = mediaDirectoryKey(path);
    const dirPref = dirKey ? directoryPrefs[dirKey] : undefined;
    const initKey = `${path}:${choices.length}:${audioTracks.length}:${JSON.stringify(dirPref ?? null)}`;
    if (initRef.current === initKey) return;

    const resolved = resolveDirectoryTrackSelection(
      path,
      choices,
      audioTracks,
      directoryPrefs,
    );

    setSubtitleChoiceId(resolved.subtitleChoiceId);
    setAudioStreamIndex(resolved.audioStreamIndex);
    initRef.current = initKey;
  }, [
    audioTracks,
    choices,
    choicesQuery.isFetched,
    directoryPrefs,
    mediaQuery.isFetched,
    mediaReady,
    path,
    setAudioStreamIndex,
    setSubtitleChoiceId,
  ]);

  useEffect(() => {
    if (!mediaReady) return;
    // 显示开关只管 overlay：隐藏时清画面，但不碰选中轨（文稿/AI 照常可用）。
    if (subtitleChoiceId === null || !subtitleVisible) {
      void setSubtitle({ source: "None" });
      return;
    }
    const choice = choices.find((c) => c.id === subtitleChoiceId);
    if (!choice) {
      const fallback = pickDefaultSubtitleId(choices);
      if (fallback !== subtitleChoiceId) {
        setSubtitleChoiceId(fallback);
      }
      return;
    }
    void applySubtitleChoice(choice, setSubtitle, path ?? undefined, loadSubtitleChoice);
  }, [
    choices,
    mediaReady,
    setSubtitle,
    setSubtitleChoiceId,
    subtitleChoiceId,
    subtitleVisible,
  ]);

  useEffect(() => {
    if (!mediaReady || audioStreamIndex == null) return;
    void setAudio(audioStreamIndex);
  }, [audioStreamIndex, mediaReady, setAudio]);

  const selectSubtitleChoiceId = (id: string | null) => {
    if (!path) {
      setSubtitleChoiceId(id);
      if (id) setSubtitleVisible(true);
      return;
    }
    const choice = id ? choices.find((c) => c.id === id) : null;
    setSubtitleChoiceId(id);
    // 显式选轨即展示（下载/切换的新轨默认上屏）；隐藏只能走显示开关。
    if (id) setSubtitleVisible(true);
    rememberSubtitleForMedia(path, choice ?? null);
  };

  const selectAudioStreamIndex = (index: number) => {
    if (!path) {
      setAudioStreamIndex(index);
      void setAudio(index);
      return;
    }
    const track = audioTracks.find((t) => t.index === index);
    setAudioStreamIndex(index);
    if (track) rememberAudioForMedia(path, track);
    void setAudio(index);
  };

  const selectedSub = choices.find((c) => c.id === subtitleChoiceId);
  const selectedAudio = audioTracks.find((t) => t.index === audioStreamIndex);

  return {
    mediaReady,
    choices,
    audioTracks,
    subtitleChoiceId,
    setSubtitleChoiceId: selectSubtitleChoiceId,
    subtitleVisible,
    setSubtitleVisible,
    audioStreamIndex,
    setAudioStreamIndex: selectAudioStreamIndex,
    setSubtitle,
    setAudio,
    selectedSub,
    selectedAudio,
  };
}
