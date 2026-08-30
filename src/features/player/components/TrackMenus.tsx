/** Subtitle + audio track menus for the player bar (bottom-right, outside HWND). */

import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { Captions, Languages } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useMediaInfoQuery } from "@/features/media";
import { listSubtitleChoices } from "@/features/transcript/api";
import type { SubtitleChoice } from "@/features/transcript/types";

import { usePlayerStore } from "../store";
import { useTrackStore } from "../trackStore";

function pickDefaultSubtitle(choices: SubtitleChoice[]): string | null {
  const text = choices.find((c) => c.supported);
  return text?.id ?? choices[0]?.id ?? null;
}

async function applySubtitleChoice(
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

function audioLabel(stream: {
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

export function TrackMenus() {
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

  if (!mediaReady) return null;

  return (
    <div className="flex items-center gap-1">
      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="max-w-[9rem] gap-1.5"
                disabled={audioTracks.length === 0}
              >
                <Languages className="size-3.5 shrink-0" />
                <span className="truncate">
                  {selectedAudio
                    ? (selectedAudio.language ?? selectedAudio.codecName ?? "音轨")
                    : "音轨"}
                </span>
              </Button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent>音轨</TooltipContent>
        </Tooltip>
        <DropdownMenuContent align="end" className="min-w-[14rem]">
          <DropdownMenuLabel>音轨</DropdownMenuLabel>
          <DropdownMenuSeparator />
          {audioTracks.map((track) => (
            <DropdownMenuCheckboxItem
              key={track.index}
              checked={audioStreamIndex === track.index}
              onCheckedChange={() => {
                setAudioStreamIndex(track.index);
                void setAudio(track.index);
              }}
            >
              {audioLabel(track)}
            </DropdownMenuCheckboxItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="max-w-[10rem] gap-1.5"
                disabled={choices.length === 0}
              >
                <Captions className="size-3.5 shrink-0" />
                <span className="truncate">
                  {selectedSub
                    ? selectedSub.label.replace(/^内嵌 · |^外挂 · /, "")
                    : "字幕"}
                </span>
              </Button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent>字幕</TooltipContent>
        </Tooltip>
        <DropdownMenuContent align="end" className="min-w-[16rem]">
          <DropdownMenuLabel>字幕</DropdownMenuLabel>
          <DropdownMenuSeparator />
          <DropdownMenuCheckboxItem
            checked={subtitleChoiceId === null}
            onCheckedChange={() => {
              setSubtitleChoiceId(null);
              void setSubtitle({ source: "None" });
            }}
          >
            关闭字幕
          </DropdownMenuCheckboxItem>
          <DropdownMenuSeparator />
          {choices.map((choice) => (
            <DropdownMenuCheckboxItem
              key={choice.id}
              checked={subtitleChoiceId === choice.id}
              onCheckedChange={() => setSubtitleChoiceId(choice.id)}
            >
              {choice.label}
            </DropdownMenuCheckboxItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
