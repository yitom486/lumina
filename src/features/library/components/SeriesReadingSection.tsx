import { useQuery } from "@tanstack/react-query";

import { usePlayerStore } from "@/features/player";
import { useProgressStore } from "@/features/player/progressStore";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { getSeriesForMedia } from "../api";
import type { EpisodeFile } from "../types";
import {
  READING_STATUS_LABEL,
  episodeReadingStatus,
  findContinueTarget,
  findNextEpisode,
  useReadingStore,
  type ReadingStatus,
} from "../readingStore";

function episodeLabel(episode: EpisodeFile): string {
  const code = `S${String(episode.season).padStart(2, "0")}E${String(episode.episode).padStart(2, "0")}`;
  return `${code} ${episode.title}`;
}

function StatusChip({ status }: { status: ReadingStatus }) {
  return (
    <span
      className={cn(
        "shrink-0 rounded px-1.5 py-0.5 text-[10px]",
        status === "done" &&
          "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
        status === "reading" && "bg-accent text-accent-foreground",
        status === "not-started" && "bg-muted text-muted-foreground",
      )}
    >
      {READING_STATUS_LABEL[status]}
    </span>
  );
}

/**
 * P6-M4 series reading shelf, merged into the library tab (not a new entry).
 * Silent when the current media is not an indexed series episode.
 */
export function SeriesReadingSection() {
  const currentFile = usePlayerStore((s) => s.currentFile);
  const status = usePlayerStore((s) => s.status);
  const openPath = usePlayerStore((s) => s.openPath);
  const progressByPath = useProgressStore((s) => s.byPath);
  const doneSet = useReadingStore((s) => s.doneSet);
  const markDone = useReadingStore((s) => s.markDone);
  const unmarkDone = useReadingStore((s) => s.unmarkDone);

  const remote = currentFile != null && /^https?:\/\//i.test(currentFile);
  // Query stays quiet on Error, but cached series data keeps rendering so a
  // failed open can be retried from the same list (recoverable, never blank).
  const queryEnabled =
    Boolean(currentFile) &&
    !remote &&
    status !== "Idle" &&
    status !== "Loading" &&
    status !== "Error";
  const seriesQuery = useQuery({
    queryKey: ["series-reading", currentFile],
    queryFn: () => getSeriesForMedia(currentFile as string),
    enabled: queryEnabled,
    retry: false,
    staleTime: Infinity,
  });

  const series = seriesQuery.data ?? null;
  if (!currentFile || remote || !series || series.episodes.length === 0) {
    return null;
  }

  const statusOf = (path: string) =>
    episodeReadingStatus(path, progressByPath[path] != null, doneSet);
  const isDone = (path: string) => statusOf(path) === "done";
  const continueTarget = findContinueTarget(series.episodes, isDone);
  const next = findNextEpisode(series.episodes, currentFile);
  const open = (path: string) => {
    void openPath(path, { rebuildPlaylist: true });
  };

  return (
    <section className="shrink-0 space-y-2 border-b border-border px-3 py-2">
      <div className="flex items-center justify-between gap-2">
        <p className="truncate text-sm font-medium">继续阅读 · {series.label}</p>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {continueTarget ? (
          <Button size="sm" onClick={() => open(continueTarget.path)}>
            继续：{episodeLabel(continueTarget)}
          </Button>
        ) : (
          <p className="text-xs text-muted-foreground">本系列已读完</p>
        )}
        {next && next.path !== continueTarget?.path ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => open(next.path)}
          >
            下一集：{episodeLabel(next)}
          </Button>
        ) : null}
      </div>
      <ul className="space-y-0.5">
        {series.episodes.map((episode) => {
          const state = statusOf(episode.path);
          const current = episode.path === currentFile;
          return (
            <li
              key={`${episode.season}-${episode.episode}`}
              className="group flex items-center gap-1.5"
            >
              <button
                type="button"
                onClick={() => open(episode.path)}
                title={current ? "正在播放" : `打开${episodeLabel(episode)}`}
                className={cn(
                  "min-w-0 flex-1 truncate rounded px-1.5 py-1 text-left text-xs",
                  current
                    ? "bg-accent font-medium text-accent-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                {episodeLabel(episode)}
              </button>
              <StatusChip status={state} />
              <button
                type="button"
                className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-muted-foreground opacity-70 hover:bg-muted hover:text-foreground group-hover:opacity-100"
                title={state === "done" ? "撤销完成" : "标记完成"}
                onClick={() =>
                  state === "done"
                    ? unmarkDone(episode.path)
                    : markDone(episode.path)
                }
              >
                {state === "done" ? "撤销" : "完成"}
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
