import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";

import { useMediaInfoQuery } from "@/features/media";
import { listSubtitleChoices } from "@/features/transcript/api";
import type { SubtitleChoice } from "@/features/transcript/types";

import { usePlayerStore } from "../store";
import { useTrackStore } from "../trackStore";

function pickDefaultSubtitle(choices: SubtitleChoice[]): string | null {
  const text = choices.find((c) => c.supported);
  return text?.id ?? choices[0]?.id ?? null;
}

export async function applySubtitleChoice(
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
  const resetForFile = useTrackStore((s) => s.resetForFile);

  const mediaReady =
    Boolean(path) &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  useEffect(() => {
    resetForFile();
  }, [path, resetForFile]);

  const choicesQuery = useQuery({
    queryKey: ["subtitleChoices", path],
    queryFn: () => listSubtitleChoices(path as string),
    enabled: mediaReady,
    retry: false,
  });

  const mediaQuery = useMediaInfoQuery();
  const audioTracks =
    mediaQuery.data?.streams.filter((s) => s.kind === "Audio") ?? [];

  useEffect(() => {
    if (!choicesQuery.data?.length) return;
    if (subtitleChoiceId != null) return;
    setSubtitleChoiceId(pickDefaultSubtitle(choicesQuery.data));
  }, [choicesQuery.data, subtitleChoiceId, setSubtitleChoiceId]);

  useEffect(() => {
    if (audioTracks.length === 0) return;
    if (audioStreamIndex != null) return;
    setAudioStreamIndex(audioTracks[0]?.index ?? null);
  }, [audioTracks, audioStreamIndex, setAudioStreamIndex]);

  useEffect(() => {
    if (!mediaReady || !subtitleChoiceId) return;
    const choice = choicesQuery.data?.find((c) => c.id === subtitleChoiceId);
    void applySubtitleChoice(choice, setSubtitle);
  }, [mediaReady, subtitleChoiceId, choicesQuery.data, setSubtitle]);

  const choices = choicesQuery.data ?? [];
  const selectedSub = choices.find((c) => c.id === subtitleChoiceId);
  const selectedAudio = audioTracks.find((t) => t.index === audioStreamIndex);

  return {
    mediaReady,
    choices,
    audioTracks,
    subtitleChoiceId,
    setSubtitleChoiceId,
    audioStreamIndex,
    setAudioStreamIndex,
    setSubtitle,
    setAudio,
    selectedSub,
    selectedAudio,
  };
}
