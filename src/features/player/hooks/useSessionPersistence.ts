/** Flush last path / directory / position on close and while playing. */

import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
    useProgressStore.getState().saveProgress(path, pos);
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

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    void getCurrentWindow()
      .onCloseRequested(() => {
        flushPlaybackPersistence();
      })
      .then((dispose) => {
        unlisten = dispose;
      })
      .catch((error) => {
        console.error("session close hook failed", error);
      });

    return () => {
      unlisten?.();
    };
  }, []);
}
