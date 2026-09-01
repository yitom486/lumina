/** Restore last opened video on cold start, paused at saved position. */

import { useEffect, useRef } from "react";

import { errorMessage } from "@/lib/format";

import { usePlayerStore } from "../store";
import { useSessionStore } from "../sessionStore";

export function useSessionRestore(): void {
  const runtimeSynced = usePlayerStore((s) => s.runtimeSynced);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const restoredRef = useRef(false);

  useEffect(() => {
    if (!runtimeSynced || restoredRef.current || currentFile) return;

    const path = useSessionStore.getState().lastPath?.trim();
    if (!path) return;

    restoredRef.current = true;

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
