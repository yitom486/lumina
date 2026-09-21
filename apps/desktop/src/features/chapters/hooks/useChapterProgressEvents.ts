import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";

import { acpWatchFeedQueryKey } from "@/features/acp/api";
import type { ChapterProgressEvent } from "../api";
import { useChapterProgressStore } from "../progressStore";

const CHAPTER_PROGRESS_EVENT = "chapter-segmentation-progress";

/**
 * One durable-commit event fans out to every projection that is derived from
 * the same SQLite write. Add future committed projections here instead of
 * giving each panel its own timer or event subscription.
 */
const COMMITTED_QUERY_KEYS = [
  ["chapter-segmentation"],
  acpWatchFeedQueryKey,
] as const;

function invalidateCommittedProjections(queryClient: QueryClient): void {
  for (const queryKey of COMMITTED_QUERY_KEYS) {
    void queryClient.invalidateQueries({ queryKey });
  }
}

/**
 * Subscribe once at the application root. Chapter work continues in Rust
 * when the panel is unmounted; this cache keeps the latest safe projection so
 * a later mount can render it immediately before the SQLite query reconciles.
 */
export function useChapterProgressEvents(): void {
  const queryClient = useQueryClient();
  const upsert = useChapterProgressStore((state) => state.upsert);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      try {
        const cleanup = await listen<ChapterProgressEvent>(
          CHAPTER_PROGRESS_EVENT,
          (event) => {
            if (cancelled) return;
            upsert(event.payload);
            if (event.payload.committed) {
              invalidateCommittedProjections(queryClient);
            }
          },
        );
        if (cancelled) {
          cleanup();
          return;
        }
        unlisten = cleanup;
      } catch {
        // The browser-only development shell has no Tauri event bridge. The
        // SQLite remains the source of truth; the event is only an invalidation
        // hint and remounts still reconcile from the database.
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient, upsert]);
}
