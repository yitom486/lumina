import { useRef, useState } from "react";
import { Volume2, VolumeX } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

import { usePlayerStore } from "../store";

type Props = {
  /** Icon + vertical slider on hover (stays inside bottom HTML chrome). */
  variant?: "horizontal" | "hover-vertical";
  className?: string;
  onHoverChange?: (hovering: boolean) => void;
};

export function VolumeControl({
  variant = "horizontal",
  className,
  onHoverChange,
}: Props) {
  const volume = usePlayerStore((s) => s.volume);
  const setVolume = usePlayerStore((s) => s.setVolume);
  const [dragging, setDragging] = useState(false);
  const [draft, setDraft] = useState(volume);
  const dragVolume = useRef(volume);

  const shown = dragging ? draft : volume;
  const muted = shown <= 0;

  const muteToggle = () => void setVolume(muted ? 100 : 0);

  if (variant === "hover-vertical") {
    return (
      <div
        className={cn("group/vol relative flex shrink-0 items-end", className)}
        onMouseEnter={() => onHoverChange?.(true)}
        onMouseLeave={() => onHoverChange?.(false)}
      >
        <div
          className={cn(
            "mr-1 flex h-0 items-center overflow-hidden opacity-0 transition-all duration-200",
            "group-hover/vol:h-20 group-hover/vol:opacity-100",
            dragging && "h-20 opacity-100",
          )}
        >
          <Slider
            orientation="vertical"
            className="h-20 w-4"
            min={0}
            max={100}
            step={1}
            value={[shown]}
            onValueChange={(vals) => {
              const next = vals[0] ?? 0;
              dragVolume.current = next;
              setDragging(true);
              setDraft(next);
            }}
            onValueCommit={(vals) => {
              const next = vals[0] ?? dragVolume.current;
              setDragging(false);
              void setVolume(next);
            }}
            aria-label="音量"
          />
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              className="text-foreground hover:bg-white/10"
              aria-label={muted ? "取消静音" : "静音"}
              onClick={muteToggle}
            >
              {muted ? (
                <VolumeX className="size-4" />
              ) : (
                <Volume2 className="size-4" />
              )}
            </Button>
          </TooltipTrigger>
          <TooltipContent>音量 · 悬停调节 (M 静音)</TooltipContent>
        </Tooltip>
      </div>
    );
  }

  return (
    <div className={cn("flex w-36 items-center gap-1", className)}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={muted ? "取消静音" : "静音"}
            onClick={muteToggle}
          >
            {muted ? (
              <VolumeX className="size-4" />
            ) : (
              <Volume2 className="size-4" />
            )}
          </Button>
        </TooltipTrigger>
        <TooltipContent>静音 (M)</TooltipContent>
      </Tooltip>
      <Slider
        className="flex-1"
        min={0}
        max={100}
        step={1}
        value={[shown]}
        onValueChange={(vals) => {
          const next = vals[0] ?? 0;
          dragVolume.current = next;
          setDragging(true);
          setDraft(next);
        }}
        onValueCommit={(vals) => {
          const next = vals[0] ?? dragVolume.current;
          setDragging(false);
          void setVolume(next);
        }}
        aria-label="音量"
      />
    </div>
  );
}
