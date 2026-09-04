/** Restore last opened **local** video on cold start, paused at saved position.
 *  Remote (YouTube/Bilibili) URLs are skipped — stream auth must be fresh via「打开链接」.
 */

import { useEffect, useRef } from "react";

import { errorMessage } from "@/lib/format";

import { usePlayerStore } from "../store";
import { useSessionStore } from "../sessionStore";

function isRemotePath(path: string): boolean {
  const lower = path.trim().toLowerCase();
  return lower.startsWith("https://") || lower.startsWith("http://");
}

export function useSessionRestore(): void {
  const runtimeSynced = usePlayerStore((s) => s.runtimeSynced);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const restoredRef = useRef(false);

  useEffect(() => {
    if (!runtimeSynced || restoredRef.current || currentFile) return;

    const path = useSessionStore.getState().lastPath?.trim();
    if (!path) return;

    restoredRef.current = true;

    if (isRemotePath(path)) {
      return;
    }

    void (async () => {
      try {
        await usePlayerStore
          .getState()
          .openPath(path, { rebuildPlaylist: true, restorePaused: true });
      } catch (error) {
        console.error("session restore failed", error);
        useSessionStore.getState().clearSession();
        usePlayerStore.getState().setStatusMessage(errorMessage(error));
      }
    })();
  }, [runtimeSynced, currentFile]);
}
