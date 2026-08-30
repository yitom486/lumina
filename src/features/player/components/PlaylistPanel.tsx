import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

import { usePlayerStore } from "../store";

function fileName(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

export function PlaylistPanel() {
  const playlist = usePlayerStore((s) => s.playlist);
  const playlistIndex = usePlayerStore((s) => s.playlistIndex);
  const busy = usePlayerStore((s) => s.busy);
  const openPath = usePlayerStore((s) => s.openPath);

  if (playlist.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 flex-col px-3 py-3 text-sm text-muted-foreground">
        <p className="text-xs leading-relaxed">
          打开任意视频后，会自动把同目录下的视频列成播放列表。
        </p>
      </div>
    );
  }

  return (
    <ScrollArea className="min-h-0 flex-1">
      <ul className="space-y-0.5 px-2 py-2">
        {playlist.map((path, index) => {
          const active = index === playlistIndex;
          return (
            <li key={path}>
              <button
                type="button"
                disabled={busy}
                className={cn(
                  "w-full rounded-md px-2 py-1.5 text-left text-sm transition-colors",
                  active
                    ? "bg-accent font-medium text-accent-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
                onClick={() => {
                  if (active) return;
                  void openPath(path, { rebuildPlaylist: false }).then(() => {
                    const store = usePlayerStore.getState();
                    if (!store.resumeToast) {
                      void store.play();
                    }
                  });
                }}
              >
                <span className="mr-2 tabular-nums text-[11px] opacity-60">
                  {index + 1}
                </span>
                <span className="break-all">{fileName(path)}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </ScrollArea>
  );
}
