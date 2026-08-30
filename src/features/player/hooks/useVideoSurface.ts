/** Keep native mpv HWND aligned with the HTML placeholder rect.
 * When no media is showing, report 0×0 so HWND hides and HTML empty-state is clickable.
 */

import { useCallback, useEffect, useRef } from "react";

import * as api from "../api";
import { setSurfaceReporter } from "../surfaceBridge";
import { usePlayerStore } from "../store";

function shouldShowNativeSurface(status: string, currentFile: string | null): boolean {
  if (!currentFile) return false;
  return (
    status === "Loading" ||
    status === "Ready" ||
    status === "Playing" ||
    status === "Paused" ||
    status === "Ended"
  );
}

export function useVideoSurface() {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const status = usePlayerStore((s) => s.status);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const showNative = shouldShowNativeSurface(status, currentFile);

  const reportBounds = useCallback(async () => {
    const el = surfaceRef.current;
    if (!el) return;

    if (!shouldShowNativeSurface(
      usePlayerStore.getState().status,
      usePlayerStore.getState().currentFile,
    )) {
      try {
        await api.setSurfaceBounds({ x: 0, y: 0, width: 0, height: 0 });
      } catch (error) {
        console.error("hide surface failed", error);
      }
      return;
    }

    const rect = el.getBoundingClientRect();
    try {
      await api.setSurfaceBounds({
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
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
  }, [reportBounds, showNative, status, currentFile]);

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

  return { surfaceRef, reportBounds, showNative };
}
