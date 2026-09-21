import { useState } from "react";
import { Captions, Languages } from "lucide-react";

import { Button } from "@lumina/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@lumina/ui/tooltip";
import { cn } from "@lumina/ui/utils";

import { audioTrackLabel, useTrackControls } from "../hooks/useTrackControls";
import { SelectionCombobox } from "./SelectionCombobox";

type Props = {
  variant?: "sidebar" | "bar";
  /** Cinema bar: keep menus inside HTML chrome (no popover over HWND). */
  cinema?: boolean;
  onMenuOpenChange?: (open: boolean) => void;
};

export function TrackControlButtons({
  variant = "sidebar",
  cinema = false,
  onMenuOpenChange,
}: Props) {
  const {
    mediaReady,
    choices,
    audioTracks,
    subtitleChoiceId,
    setSubtitleChoiceId,
    subtitleVisible,
    setSubtitleVisible,
    audioStreamIndex,
    setAudioStreamIndex,
    selectedSub,
    selectedAudio,
  } = useTrackControls();

  const [audioInlineOpen, setAudioInlineOpen] = useState(false);
  const [subtitleInlineOpen, setSubtitleInlineOpen] = useState(false);

  if (!mediaReady) {
    if (variant === "bar") return null;
    return (
      <div className="px-3 py-1.5 text-[11px] text-muted-foreground">
        打开视频后可切换音轨 / 字幕 / 倍速
      </div>
    );
  }

  const compact = variant === "bar";
  const btnClass = cn(
    compact && "h-8 gap-1 border-player-control/50 bg-player-surface/40 px-2 text-xs text-player-control-foreground hover:bg-player-control-hover/60",
    compact ? "max-w-[7rem]" : "max-w-[9rem] gap-1.5",
  );

  const notifyPin = (open: boolean) => {
    onMenuOpenChange?.(open);
  };

  const toggleAudioInline = () => {
    setAudioInlineOpen((value) => {
      const next = !value;
      if (next) setSubtitleInlineOpen(false);
      notifyPin(next || subtitleInlineOpen);
      return next;
    });
  };

  const toggleSubtitleInline = () => {
    setSubtitleInlineOpen((value) => {
      const next = !value;
      if (next) setAudioInlineOpen(false);
      notifyPin(next || audioInlineOpen);
      return next;
    });
  };

  const closeInline = () => {
    setAudioInlineOpen(false);
    setSubtitleInlineOpen(false);
    notifyPin(false);
  };

  const audioButton = (
    <Button
      type="button"
      variant={compact ? "outline" : "outline"}
      size={compact ? "sm" : "sm"}
      className={cn(btnClass, compact && "max-w-[6rem]")}
      disabled={audioTracks.length === 0}
      onClick={cinema ? toggleAudioInline : undefined}
      aria-expanded={cinema ? audioInlineOpen : undefined}
    >
      <Languages className="size-3.5 shrink-0" />
      <span className="truncate">
        {selectedAudio
          ? (selectedAudio.language ?? selectedAudio.codecName ?? "音轨")
          : "音轨"}
      </span>
    </Button>
  );

  const subtitleButton = (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className={cn(btnClass, compact && "max-w-[6.5rem]")}
      disabled={choices.length === 0}
      onClick={cinema ? toggleSubtitleInline : undefined}
      aria-expanded={cinema ? subtitleInlineOpen : undefined}
    >
      <Captions className="size-3.5 shrink-0" />
      <span className="truncate">
        {selectedSub
          ? `${selectedSub.label.replace(/^内嵌 · |^外挂 · |^下载 · /, "")}${subtitleVisible ? "" : " · 已隐藏"}`
          : "字幕"}
      </span>
    </Button>
  );

  return (
    <>
      <div className={cn("flex items-center gap-1", compact && "shrink-0")}>
        {cinema ? (
          <>
            <Tooltip>
              <TooltipTrigger asChild>{audioButton}</TooltipTrigger>
              <TooltipContent>音轨</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>{subtitleButton}</TooltipTrigger>
              <TooltipContent>字幕</TooltipContent>
            </Tooltip>
          </>
        ) : (
          <>
            <SelectionCombobox
              value={audioStreamIndex == null ? null : String(audioStreamIndex)}
              options={audioTracks.map((track) => ({
                value: String(track.index),
                label: audioTrackLabel(track),
                keywords: `${track.language ?? ""} ${track.codecName ?? ""}`,
              }))}
              placeholder="音轨"
              ariaLabel="选择音轨"
              triggerLabel={
                selectedAudio
                  ? (selectedAudio.language ?? selectedAudio.codecName ?? "音轨")
                  : "音轨"
              }
              leadingIcon={<Languages className="size-3.5 shrink-0" />}
              disabled={audioTracks.length === 0}
              className={cn(btnClass, compact && "max-w-[6rem]")}
              onValueChange={(value) => setAudioStreamIndex(Number(value))}
              onOpenChange={notifyPin}
            />
            <SelectionCombobox
              value={subtitleChoiceId}
              options={choices.map((choice) => ({
                value: choice.id,
                label: choice.label,
                keywords: `${choice.language ?? ""} ${choice.codecName ?? ""}`,
              }))}
              placeholder="字幕"
              ariaLabel="选择字幕"
              triggerLabel={
                selectedSub
                  ? `${selectedSub.label.replace(/^内嵌 · |^外挂 · |^下载 · /, "")}${subtitleVisible ? "" : " · 已隐藏"}`
                  : "字幕"
              }
              leadingIcon={<Captions className="size-3.5 shrink-0" />}
              disabled={choices.length === 0}
              className={cn(btnClass, compact && "max-w-[6.5rem]")}
              onValueChange={setSubtitleChoiceId}
              onOpenChange={notifyPin}
              toggle={{
                label: "显示字幕",
                checked: subtitleVisible,
                onChange: () => setSubtitleVisible(!subtitleVisible),
              }}
            />
          </>
        )}
      </div>

      {cinema && (audioInlineOpen || subtitleInlineOpen) ? (
        <div className="flex flex-wrap gap-1 border-t border-player-control/50 px-3 py-2">
          {audioInlineOpen
            ? audioTracks.map((track) => (
                <Button
                  key={track.index}
                  type="button"
                  size="sm"
                  variant={
                    audioStreamIndex === track.index ? "secondary" : "ghost"
                  }
                  className="h-7 text-xs"
                  onClick={() => {
                    setAudioStreamIndex(track.index);
                    closeInline();
                  }}
                >
                  {audioTrackLabel(track)}
                </Button>
              ))
            : null}
          {subtitleInlineOpen ? (
            <>
              <Button
                type="button"
                size="sm"
                variant={subtitleVisible ? "secondary" : "ghost"}
                className="h-7 text-xs"
                onClick={() => {
                  setSubtitleVisible(!subtitleVisible);
                  closeInline();
                }}
              >
                {subtitleVisible ? "隐藏字幕" : "显示字幕"}
              </Button>
              {choices.map((choice) => (
                <Button
                  key={choice.id}
                  type="button"
                  size="sm"
                  variant={
                    subtitleChoiceId === choice.id ? "secondary" : "ghost"
                  }
                  className="h-7 max-w-full truncate text-xs"
                  onClick={() => {
                    setSubtitleChoiceId(choice.id);
                    closeInline();
                  }}
                >
                  {choice.label}
                </Button>
              ))}
            </>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
