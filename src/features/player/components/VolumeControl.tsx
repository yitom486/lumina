import { useRef, useState } from "react";
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
  const [dragging, setDragging] = useState(false);
  const [draft, setDraft] = useState(volume);
  const dragVolume = useRef(volume);

  const shown = dragging ? draft : volume;
  const muted = shown <= 0;

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
