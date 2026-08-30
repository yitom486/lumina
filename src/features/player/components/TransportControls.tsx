import { FolderOpen, Pause, Play, Square } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { usePlayerStore } from "../store";

export function TransportControls() {
  const busy = usePlayerStore((s) => s.busy);
  const status = usePlayerStore((s) => s.status);
  const openFile = usePlayerStore((s) => s.openFile);
  const togglePlayPause = usePlayerStore((s) => s.togglePlayPause);
  const stop = usePlayerStore((s) => s.stop);

  const playing = status === "Playing";
  const canToggle =
    !busy &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";

  return (
    <div className="flex items-center gap-1">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="secondary"
            size="icon-sm"
            onClick={() => void openFile()}
            disabled={busy}
            aria-label="打开文件"
          >
            <FolderOpen className="size-4" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>打开视频</TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="default"
            size="icon-sm"
            onClick={() => void togglePlayPause()}
            disabled={!canToggle}
            aria-label={playing ? "暂停" : "播放"}
          >
            {playing ? (
              <Pause className="size-4" />
            ) : (
              <Play className="size-4" />
            )}
          </Button>
        </TooltipTrigger>
        <TooltipContent>{playing ? "暂停 (空格)" : "播放 (空格)"}</TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => void stop()}
            disabled={busy || status === "Idle"}
            aria-label="停止"
          >
            <Square className="size-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>停止</TooltipContent>
      </Tooltip>
    </div>
  );
}
