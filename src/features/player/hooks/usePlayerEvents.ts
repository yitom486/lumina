/** Wire Tauri Channel → Zustand. Surface click/dblclick → play/fullscreen.
 * On mount / HMR reload, pull authoritative snapshot from Rust so UI does not
 * show empty-state while mpv is still playing.
 */

import { useEffect, useRef } from "react";
import { Channel } from "@tauri-apps/api/core";

import { errorMessage } from "@/lib/format";

import * as api from "../api";
import { ensureSurfaceBounds } from "../surfaceBridge";
import { usePlayerStore } from "../store";
import { useUiStore } from "../uiStore";
import type { PlayerEvent } from "../types";

const CLICK_DELAY_MS = 280;

export function usePlayerEvents(): void {
  const clickTimer = useRef<number | null>(null);
  const suppressClickUntil = useRef(0);

  useEffect(() => {
    const clearClickTimer = () => {
      if (clickTimer.current != null) {
        window.clearTimeout(clickTimer.current);
        clickTimer.current = null;
      }
    };

    const onEvent = new Channel<PlayerEvent>();
    onEvent.onmessage = (event) => {
      if (event.type === "SurfaceDoubleClick") {
        clearClickTimer();
        // Swallow the trailing LBUTTONUP that follows a double-click.
        suppressClickUntil.current = Date.now() + 400;
        void useUiStore.getState().toggleFullscreen();
        return;
      }
      if (event.type === "SurfaceClick") {
        if (Date.now() < suppressClickUntil.current) {
          return;
        }
        clearClickTimer();
        clickTimer.current = window.setTimeout(() => {
          clickTimer.current = null;
          const store = usePlayerStore.getState();
          if (
            store.status === "Playing" ||
            store.status === "Paused" ||
            store.status === "Ready" ||
            store.status === "Ended"
          ) {
            void store.togglePlayPause();
          }
        }, CLICK_DELAY_MS);
        return;
      }
      usePlayerStore.getState().applyEvent(event);
    };

    let cancelled = false;
    let syncVersion = 0;

    const syncFromRuntime = async () => {
      const version = ++syncVersion;
      const store = usePlayerStore.getState();
      store.setRuntimeSynced(false);

      try {
        const snapshot = await api.getPlayerState();
        if (cancelled || version !== syncVersion) return;
        usePlayerStore.getState().applySnapshot(snapshot);
        usePlayerStore.getState().setRuntimeSynced(true);
        if (snapshot.currentFile) {
          usePlayerStore.setState({
            statusMessage:
              snapshot.currentFile.split(/[/\\]/).pop() ?? snapshot.currentFile,
          });
        }
        requestAnimationFrame(() => {
          void ensureSurfaceBounds();
        });
      } catch (error) {
        if (cancelled || version !== syncVersion) return;
        console.error("player state resync failed", error);
        usePlayerStore.getState().setStatusMessage(errorMessage(error));
      }
    };

    const onViteAfterUpdate = () => {
      void syncFromRuntime();
    };

    import.meta.hot?.on("vite:afterUpdate", onViteAfterUpdate);

    void (async () => {
      try {
        await api.subscribePlayerEvents(onEvent);
        if (cancelled) return;
        await syncFromRuntime();
        window.setTimeout(() => {
          void ensureSurfaceBounds();
        }, 80);
      } catch (error) {
        console.error("player_subscribe / resync failed", error);
        usePlayerStore.getState().setStatusMessage(errorMessage(error));
      }
    })();

    return () => {
      cancelled = true;
      syncVersion += 1;
      clearClickTimer();
      import.meta.hot?.off("vite:afterUpdate", onViteAfterUpdate);
    };
  }, []);
}
