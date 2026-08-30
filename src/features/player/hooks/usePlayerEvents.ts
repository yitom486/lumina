/** Wire Tauri Channel → Zustand. Call once at app root. */

import { useEffect } from "react";
import { Channel } from "@tauri-apps/api/core";

import { errorMessage } from "@/lib/format";

import * as api from "../api";
import { usePlayerStore } from "../store";
import type { PlayerEvent } from "../types";

export function usePlayerEvents(): void {
  useEffect(() => {
    const onEvent = new Channel<PlayerEvent>();
    onEvent.onmessage = (event) => {
      usePlayerStore.getState().applyEvent(event);
    };

    void api.subscribePlayerEvents(onEvent).catch((error) => {
      console.error("player_subscribe failed", error);
      usePlayerStore.getState().setStatusMessage(errorMessage(error));
    });
  }, []);
}
