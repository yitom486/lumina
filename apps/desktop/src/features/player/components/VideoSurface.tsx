/** Transparent placeholder; actual pixels come from native libmpv HWND.
 * Empty state (no media): HWND is hidden so this HTML layer is clickable.
 */

import { FolderOpen } from "lucide-react";

import { formatPlayerError } from "@/lib/format";
import { usePlayerStore } from "../store";
import { useVideoSurface } from "../hooks/useVideoSurface";

export function VideoSurface() {
  const { surfaceRef, showEmpty } = useVideoSurface();
  const openFile = usePlayerStore((s) => s.openFile);
  const busy = usePlayerStore((s) => s.busy);
  const status = usePlayerStore((s) => s.status);
  const error = usePlayerStore((s) => s.error);

  const errorText = formatPlayerError(error);

  return (
    <div
      ref={surfaceRef}
      className="relative min-h-0 flex-1 bg-black"
      aria-label="Native video surface"
    >
      {showEmpty ? (
        <button
          type="button"
          disabled={busy}
          onClick={() => void openFile()}
          className="absolute inset-0 flex flex-col items-center justify-center gap-4 text-muted-foreground transition-colors hover:bg-white/5 hover:text-foreground disabled:opacity-60"
        >
          <FolderOpen className="size-24 stroke-[1.25]" aria-hidden />
          <div className="px-6 text-center">
            <p className="text-base font-medium text-foreground">
              {status === "Error" ? "打开失败，点击重新选择" : "打开视频"}
            </p>
            <p className="mt-1 max-w-sm text-sm text-muted-foreground">
              {status === "Error"
                ? errorText
                : "点击选择本地文件，同目录视频会加入播放列表"}
            </p>
          </div>
        </button>
      ) : null}
    </div>
  );
}
