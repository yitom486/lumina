import {
  FolderOpen,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Square,
} from "lucide-react";

import { Button } from "@lumina/ui/button";

import { usePlayerStore } from "../store";
import { OpenUrlButton } from "./OpenUrlButton";

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

  const playing = status === "Playing";
  const canToggle =
    !busy &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";
  const canPrev = !busy && playlistIndex > 0;
  const canNext =
    !busy && playlistIndex >= 0 && playlistIndex < playlist.length - 1;

  return (
    <div className="flex items-center gap-1">
      <Button
        type="button"
        variant="secondary"
        size="icon" className="size-8"
        onClick={() => void openFile()}
        disabled={busy}
        aria-label="打开文件"
      >
        <FolderOpen className="size-4" />
      </Button>

      <OpenUrlButton />

      <Button
        type="button"
        variant="ghost"
        size="icon" className="size-8"
        onClick={() => void playPrev()}
        disabled={!canPrev}
        aria-label="上一个"
      >
        <SkipBack className="size-4" />
      </Button>

      <Button
        type="button"
        variant="default"
        size="icon" className="size-8"
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

      <Button
        type="button"
        variant="ghost"
        size="icon" className="size-8"
        onClick={() => void playNext()}
        disabled={!canNext}
        aria-label="下一个"
      >
        <SkipForward className="size-4" />
      </Button>

      <Button
        type="button"
        variant="ghost"
        size="icon" className="size-8"
        onClick={() => void stop()}
        disabled={busy || status === "Idle"}
        aria-label="停止"
      >
        <Square className="size-3.5" />
      </Button>
    </div>
  );
}
