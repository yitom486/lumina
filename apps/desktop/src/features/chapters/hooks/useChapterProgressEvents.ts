import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";

import type { ChapterProgressEvent } from "../api";
import { useChapterProgressStore } from "../progressStore";

const CHAPTER_PROGRESS_EVENT = "chapter-segmentation-progress";

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
              void queryClient.invalidateQueries({
                queryKey: ["chapter-segmentation"],
              });
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
        // SQLite polling path remains the fallback and source of truth.
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient, upsert]);
}
