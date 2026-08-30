import {
  FolderOpen,
  Maximize2,
  Minimize2,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Square,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { usePlayerStore } from "../store";
import { useUiStore } from "../uiStore";

export function TransportControls() {
  const busy = usePlayerStore((s) => s.busy);
  const status = usePlayerStore((s) => s.status);
  const playlist = usePlayerStore((s) => s.playlist);
  const playlistIndex = usePlayerStore((s) => s.playlistIndex);
  const openFile = usePlayerStore((s) => s.openFile);
  const togglePlayPause = usePlayerStore((s) => s.togglePlayPause);
  const stop = usePlayerStore((s) => s.stop);
  const playNext = usePlayerStore((s) => s.playNext);
  const playPrev = usePlayerStore((s) => s.playPrev);
  const fullscreen = useUiStore((s) => s.fullscreen);
  const toggleFullscreen = useUiStore((s) => s.toggleFullscreen);

  const playing = status === "Playing";
  const canToggle =
    !busy &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";
  const canPrev = !busy && playlistIndex > 0;
  const canNext = !busy && playlistIndex >= 0 && playlistIndex < playlist.length - 1;

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
        <TooltipContent>打开视频（同目录加入播放列表）</TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => void playPrev()}
            disabled={!canPrev}
            aria-label="上一个"
          >
            <SkipBack className="size-4" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>上一个</TooltipContent>
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
            onClick={() => void playNext()}
            disabled={!canNext}
            aria-label="下一个"
          >
            <SkipForward className="size-4" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>下一个</TooltipContent>
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

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => void toggleFullscreen()}
            aria-label={fullscreen ? "退出全屏" : "全屏"}
          >
            {fullscreen ? (
              <Minimize2 className="size-4" />
            ) : (
              <Maximize2 className="size-4" />
            )}
          </Button>
        </TooltipTrigger>
        <TooltipContent>{fullscreen ? "退出全屏 (Esc / F)" : "全屏 (F)"}</TooltipContent>
      </Tooltip>
    </div>
  );
}
