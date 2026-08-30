import { Volume2, VolumeX } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { usePlayerStore } from "../store";

export function VolumeControl() {
  const volume = usePlayerStore((s) => s.volume);
  const setVolume = usePlayerStore((s) => s.setVolume);

  const muted = volume <= 0;

  return (
    <div className="flex w-36 items-center gap-1">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={muted ? "取消静音" : "静音"}
            onClick={() => void setVolume(muted ? 100 : 0)}
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
        value={[volume]}
        onValueChange={(vals) => void setVolume(vals[0] ?? 0)}
        aria-label="音量"
      />
    </div>
  );
}
