/** Throttle progress writes while playing; flush on pause/end. */

import { useEffect, useRef } from "react";

import { flushPlaybackPersistence } from "./hooks/useSessionPersistence";
import { usePlayerStore } from "../store";
import { useProgressStore } from "../progressStore";

const SAVE_INTERVAL_MS = 2_500;

export function useProgressPersistence() {
  const status = usePlayerStore((s) => s.status);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const currentTimeMs = usePlayerStore((s) => s.currentTimeMs);
  const saveProgress = useProgressStore((s) => s.saveProgress);
  const clearProgress = useProgressStore((s) => s.clearProgress);
  const lastSavedAt = useRef(0);

  useEffect(() => {
    if (!currentFile) return;

    if (status === "Ended") {
      clearProgress(currentFile);
      return;
    }

    if (status === "Paused" || status === "Ready") {
      saveProgress(currentFile, currentTimeMs);
      lastSavedAt.current = Date.now();
      return;
    }

    if (status !== "Playing") return;

    const now = Date.now();
    if (now - lastSavedAt.current >= SAVE_INTERVAL_MS) {
      saveProgress(currentFile, currentTimeMs);
      lastSavedAt.current = now;
    }
  }, [status, currentFile, currentTimeMs, saveProgress, clearProgress]);

  useEffect(() => {
    window.addEventListener("beforeunload", flushPlaybackPersistence);
    return () =>
      window.removeEventListener("beforeunload", flushPlaybackPersistence);
  }, []);
}
