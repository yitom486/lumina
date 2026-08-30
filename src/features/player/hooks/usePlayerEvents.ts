/** Wire Tauri Channel → Zustand. Surface click/dblclick → play/fullscreen. */

import { useEffect, useRef } from "react";
import { Channel } from "@tauri-apps/api/core";

import { errorMessage } from "@/lib/format";

import * as api from "../api";
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

    void api.subscribePlayerEvents(onEvent).catch((error) => {
      console.error("player_subscribe failed", error);
      usePlayerStore.getState().setStatusMessage(errorMessage(error));
    });

    return () => clearClickTimer();
  }, []);
}
