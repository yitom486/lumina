/** Keep native mpv HWND aligned with the HTML placeholder rect ONLY.
 * Player bar / sidebar are pure HTML and must never be covered by HWND.
 */

import { useCallback, useEffect, useRef } from "react";

import * as api from "../api";
import { setSurfaceReporter } from "../surfaceBridge";
import { usePlayerStore } from "../store";

export type NativeSurfaceMode = "preserve" | "show" | "hide";

/** CSS cannot order against HWND. Do not mutate it until Rust state is known. */
export function nativeSurfaceMode(
  runtimeSynced: boolean,
  status: string,
  currentFile: string | null,
): NativeSurfaceMode {
  if (!runtimeSynced) return "preserve";
  if (!currentFile) return "hide";
  return (
    status === "Loading" ||
    status === "Ready" ||
    status === "Playing" ||
    status === "Paused" ||
    status === "Ended"
  )
    ? "show"
    : "hide";
}

export function useVideoSurface() {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const runtimeSynced = usePlayerStore((s) => s.runtimeSynced);
  const status = usePlayerStore((s) => s.status);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const mode = nativeSurfaceMode(runtimeSynced, status, currentFile);

  const reportBounds = useCallback(async () => {
    const el = surfaceRef.current;
    if (!el) return;

    const state = usePlayerStore.getState();
    const nextMode = nativeSurfaceMode(
      state.runtimeSynced,
      state.status,
      state.currentFile,
    );

    // During mount/HMR, the native player can still be showing valid pixels.
    // A stale WebView Idle state must not collapse that HWND to 0x0.
    if (nextMode === "preserve") return;

    if (nextMode === "hide") {
      try {
        await api.setSurfaceBounds({ x: 0, y: 0, width: 0, height: 0 });
      } catch (error) {
        console.error("hide surface failed", error);
      }
      return;
    }

    const rect = el.getBoundingClientRect();
    const width = Math.round(rect.width);
    const height = Math.round(rect.height);
    if (width < 2 || height < 2) {
      try {
        await api.setSurfaceBounds({ x: 0, y: 0, width: 0, height: 0 });
      } catch (error) {
        console.error("hide tiny surface failed", error);
      }
      return;
    }

    try {
      await api.setSurfaceBounds({
        x: Math.round(rect.left),
        y: Math.round(rect.top),
        width,
        height,
      });
    } catch (error) {
      console.error("set surface bounds failed", error);
    }
  }, []);

  useEffect(() => {
    setSurfaceReporter(reportBounds);
    return () => setSurfaceReporter(null);
  }, [reportBounds]);

  useEffect(() => {
    void reportBounds();
  }, [reportBounds, mode, status, currentFile]);

  useEffect(() => {
    void reportBounds();
    const el = surfaceRef.current;
    if (!el) return;

    const observer = new ResizeObserver(() => {
      void reportBounds();
    });
    observer.observe(el);
    window.addEventListener("resize", reportBounds);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", reportBounds);
    };
  }, [reportBounds]);

  return {
    surfaceRef,
    reportBounds,
    showNative: mode === "show",
    showEmpty: mode === "hide",
  };
}
