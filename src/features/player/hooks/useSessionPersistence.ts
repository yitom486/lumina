/** Flush last path / directory / position on close and while playing. */

import { useEffect } from "react";

import { usePlayerStore } from "../store";
import { useProgressStore } from "../progressStore";
import { parentDirectory, useSessionStore } from "../sessionStore";

export function flushPlaybackPersistence(): void {
  const { currentFile: path, currentTimeMs: pos, status } =
    usePlayerStore.getState();
  if (!path) return;

  if (status === "Ended") {
    useProgressStore.getState().clearProgress(path);
    useSessionStore.getState().clearSession();
    return;
  }

  if (status === "Playing" || status === "Paused" || status === "Ready") {
    // Avoid binding another file's ghost position onto a fresh remote URL.
    const { durationMs, sourceKind } = usePlayerStore.getState();
    if (!(sourceKind === "remote" && durationMs <= 0)) {
      useProgressStore.getState().saveProgress(path, pos);
    }
    useSessionStore.getState().saveSession({
      path,
      positionMs: pos,
      directory: parentDirectory(path),
    });
  }
}

export function useSessionPersistence(): void {
  const status = usePlayerStore((s) => s.status);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);

  useEffect(() => {
    if (!currentFile) return;
    if (status === "Playing" || status === "Paused" || status === "Ready") {
      const { durationMs, sourceKind } = usePlayerStore.getState();
      if (sourceKind === "remote" && durationMs <= 0) return;
      useSessionStore.getState().saveSession({
        path: currentFile,
        positionMs: currentTimeMs,
        directory: parentDirectory(currentFile),
      });
    }
  }, [status, currentFile, currentTimeMs]);

  useEffect(() => {
    window.addEventListener("beforeunload", flushPlaybackPersistence);
    return () => window.removeEventListener("beforeunload", flushPlaybackPersistence);
  }, []);

}
