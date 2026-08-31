import { useState } from "react";
import { Captions, Languages } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

import { audioTrackLabel, useTrackControls } from "../hooks/useTrackControls";

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
    audioStreamIndex,
    setAudioStreamIndex,
    setSubtitle,
    setAudio,
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
    compact && "h-8 gap-1 border-white/15 bg-black/40 px-2 text-xs text-foreground hover:bg-white/10",
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
          ? selectedSub.label.replace(/^内嵌 · |^外挂 · /, "")
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
            <DropdownMenu onOpenChange={notifyPin}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <DropdownMenuTrigger asChild>{audioButton}</DropdownMenuTrigger>
                </TooltipTrigger>
                <TooltipContent>音轨</TooltipContent>
              </Tooltip>
              <DropdownMenuContent align="start" side="bottom" className="min-w-[14rem]">
                <DropdownMenuLabel>音轨</DropdownMenuLabel>
                <DropdownMenuSeparator />
                {audioTracks.length === 0 ? (
                  <DropdownMenuItem disabled>没有音轨</DropdownMenuItem>
                ) : null}
                {audioTracks.map((track) => (
                  <DropdownMenuCheckboxItem
                    key={track.index}
                    checked={audioStreamIndex === track.index}
                    onCheckedChange={() => {
                      setAudioStreamIndex(track.index);
                      void setAudio(track.index);
                    }}
                  >
                    {audioTrackLabel(track)}
                  </DropdownMenuCheckboxItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>

            <DropdownMenu onOpenChange={notifyPin}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <DropdownMenuTrigger asChild>{subtitleButton}</DropdownMenuTrigger>
                </TooltipTrigger>
                <TooltipContent>字幕</TooltipContent>
              </Tooltip>
              <DropdownMenuContent align="start" side="bottom" className="min-w-[16rem]">
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
          </>
        )}
      </div>

      {cinema && (audioInlineOpen || subtitleInlineOpen) ? (
        <div className="flex flex-wrap gap-1 border-t border-white/10 px-3 py-2">
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
                    void setAudio(track.index);
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
                variant={subtitleChoiceId === null ? "secondary" : "ghost"}
                className="h-7 text-xs"
                onClick={() => {
                  setSubtitleChoiceId(null);
                  void setSubtitle({ source: "None" });
                  closeInline();
                }}
              >
                关闭字幕
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
