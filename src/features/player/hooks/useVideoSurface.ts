/** Keep native mpv HWND aligned with the HTML placeholder rect. */

import { useCallback, useEffect, useRef } from "react";

import * as api from "../api";
import { setSurfaceReporter } from "../surfaceBridge";

export function useVideoSurface() {
  const surfaceRef = useRef<HTMLDivElement>(null);

  const reportBounds = useCallback(async () => {
    const el = surfaceRef.current;
    if (!el) return;
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

  return { surfaceRef, reportBounds };
}
