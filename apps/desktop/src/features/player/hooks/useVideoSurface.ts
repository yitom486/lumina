/** Keep native mpv HWND aligned with the HTML placeholder rect ONLY.
 * Player bar / sidebar are pure HTML and must never be covered by HWND.
 */

import { useCallback, useEffect, useLayoutEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { useChatUiStore } from "@/features/acp/chatUiStore";

import * as api from "../api";
import { setSurfaceReporter } from "../surfaceBridge";
import { createLatestBoundsQueue } from "../surfaceBoundsQueue";
import { usePlayerStore } from "../store";
import { useUiStore } from "../uiStore";

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

/** Fullscreen / sidebar / dock reflow can commit after the OS resize — retry briefly. */
function scheduleBoundsRefresh(report: () => Promise<void>): () => void {
  void report();

  let raf1 = 0;
  let raf2 = 0;
  raf1 = requestAnimationFrame(() => {
    void report();
    raf2 = requestAnimationFrame(() => {
      void report();
    });
  });

  const timers = [100, 300, 600].map((ms) =>
    window.setTimeout(() => {
      void report();
    }, ms),
  );

  return () => {
    cancelAnimationFrame(raf1);
    cancelAnimationFrame(raf2);
    for (const id of timers) {
      window.clearTimeout(id);
    }
  };
}

export function useVideoSurface() {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const queueBoundsRef = useRef<ReturnType<typeof createLatestBoundsQueue> | null>(null);
  const runtimeSynced = usePlayerStore((s) => s.runtimeSynced);
  const status = usePlayerStore((s) => s.status);
  const currentFile = usePlayerStore((s) => s.currentFile);
  const fullscreen = useUiStore((s) => s.fullscreen);
  const chatOpen = useChatUiStore((s) => s.chatOpen);
  const mode = nativeSurfaceMode(runtimeSynced, status, currentFile);

  if (!queueBoundsRef.current) {
    queueBoundsRef.current = createLatestBoundsQueue(
      api.setSurfaceBounds,
      (error) => console.error("set surface bounds failed", error),
    );
  }

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
      await queueBoundsRef.current?.({ x: 0, y: 0, width: 0, height: 0 });
      return;
    }

    const rect = el.getBoundingClientRect();
    const width = Math.round(rect.width);
    const height = Math.round(rect.height);
    if (width < 2 || height < 2) {
      await queueBoundsRef.current?.({ x: 0, y: 0, width: 0, height: 0 });
      return;
    }

    await queueBoundsRef.current?.({
      x: Math.round(rect.left),
      y: Math.round(rect.top),
      width,
      height,
    });
  }, []);

  useEffect(() => {
    setSurfaceReporter(reportBounds);
    return () => setSurfaceReporter(null);
  }, [reportBounds]);

  useLayoutEffect(() => {
    return scheduleBoundsRefresh(reportBounds);
  }, [reportBounds, fullscreen, chatOpen]);

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

    let unlistenResize: (() => void) | undefined;
    void getCurrentWindow()
      .onResized(() => {
        scheduleBoundsRefresh(reportBounds);
      })
      .then((unlisten) => {
        unlistenResize = unlisten;
      })
      .catch((error) => {
        console.error("window onResized subscribe failed", error);
      });

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", reportBounds);
      unlistenResize?.();
    };
  }, [reportBounds]);

  return {
    surfaceRef,
    reportBounds,
    showNative: mode === "show",
    showEmpty: mode === "hide",
  };
}
