import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";

import { useMediaInfoQuery } from "@/features/media";
import {
  listSubtitleChoices,
  loadSubtitleChoice,
} from "@/features/transcript/api";
import type { SubtitleChoice } from "@/features/transcript/types";

import { usePlayerStore } from "../store";
import {
  mediaDirectoryKey,
  resolveDirectoryTrackSelection,
  pickDefaultSubtitleId,
} from "../trackPreferences";
import { useTrackStore } from "../trackStore";

export async function applySubtitleChoice(
  choice: SubtitleChoice | undefined,
  setSubtitle: (args: {
    source: "Embedded" | "Sidecar" | "None";
    streamIndex?: number | null;
    externalPath?: string | null;
  }) => Promise<void>,
  mediaPath?: string,
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
  if (choice.id.startsWith("online:") && !choice.externalPath) {
    if (!mediaPath) return;
    const transcript = await loadSubtitleChoice(mediaPath, choice.id);
    await setSubtitle({
      source: "Sidecar",
      externalPath: transcript.sourcePath,
    });
    return;
  }
  await setSubtitle({
    source: "Sidecar",
    externalPath: choice.externalPath,
  });
}

export function audioTrackLabel(stream: {
  index: number;
  language?: string | null;
  codecName?: string | null;
  channels?: number | null;
}): string {
  const lang = stream.language ?? "und";
  const codec = stream.codecName ?? "audio";
  const ch = stream.channels ? `${stream.channels}ch` : null;
  return [lang, codec, ch].filter(Boolean).join(" · ");
}

export function useTrackControls() {
  const path = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const setSubtitle = usePlayerStore((s) => s.setSubtitle);
  const setAudio = usePlayerStore((s) => s.setAudio);

  const subtitleChoiceId = useTrackStore((s) => s.subtitleChoiceId);
  const setSubtitleChoiceId = useTrackStore((s) => s.setSubtitleChoiceId);
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
    queryKey: ["subtitleChoices", path],
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
    if (subtitleChoiceId === null) {
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
    void applySubtitleChoice(choice, setSubtitle, path ?? undefined);
  }, [
    choices,
    mediaReady,
    setSubtitle,
    setSubtitleChoiceId,
    subtitleChoiceId,
  ]);

  useEffect(() => {
    if (!mediaReady || audioStreamIndex == null) return;
    void setAudio(audioStreamIndex);
  }, [audioStreamIndex, mediaReady, setAudio]);

  const selectSubtitleChoiceId = (id: string | null) => {
    if (!path) {
      setSubtitleChoiceId(id);
      return;
    }
    const choice = id ? choices.find((c) => c.id === id) : null;
    setSubtitleChoiceId(id);
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
    audioStreamIndex,
    setAudioStreamIndex: selectAudioStreamIndex,
    setSubtitle,
    setAudio,
    selectedSub,
    selectedAudio,
  };
}
