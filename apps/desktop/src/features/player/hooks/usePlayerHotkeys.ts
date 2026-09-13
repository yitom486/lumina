/** Global player hotkeys. Ignores typing in form fields. */

import { useEffect, useRef } from "react";

import { useChatUiStore } from "@/features/acp/chatUiStore";
import { usePlayerStore } from "../store";
import { useUiStore } from "../uiStore";
import { shouldSuppressPlayerHotkeyDuringAcp } from "./playerHotkeyPolicy";

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if (target.isContentEditable) return true;
  return false;
}

export function usePlayerHotkeys() {
  const volumeBeforeMute = useRef(100);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (isTypingTarget(event.target)) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;

      const store = usePlayerStore.getState();
      const ui = useUiStore.getState();
      const chat = useChatUiStore.getState();

      if (event.key === "Escape") {
        if (chat.chatOpen) {
          event.preventDefault();
          chat.closeChat();
          return;
        }
        if (chat.acpResponding) {
          event.preventDefault();
          return;
        }
        if (ui.fullscreen) {
          event.preventDefault();
          void ui.setFullscreen(false);
          return;
        }
      }

      if (shouldSuppressPlayerHotkeyDuringAcp(event.code, chat.acpResponding)) {
        event.preventDefault();
        return;
      }

      const busy = store.busy;
      const status = store.status;
      const ready =
        status !== "Idle" && status !== "Loading" && status !== "Error";

      switch (event.code) {
        case "Space": {
          event.preventDefault();
          if (busy || !ready) return;
          if (status === "Playing") {
            void store.pause();
          } else {
            void store.play();
          }
          break;
        }
        case "ArrowLeft": {
          event.preventDefault();
          if (!ready) return;
          void store.seek(Math.max(0, store.currentTimeMs - 5_000));
          break;
        }
        case "ArrowRight": {
          event.preventDefault();
          if (!ready) return;
          const duration = store.durationMs;
          const next = store.currentTimeMs + 5_000;
          // Allow skip even when duration is still unknown (mpv demux lag).
          void store.seek(duration > 0 ? Math.min(duration, next) : next);
          break;
        }
        case "ArrowUp": {
          event.preventDefault();
          void store.setVolume(Math.min(100, store.volume + 5));
          break;
        }
        case "ArrowDown": {
          event.preventDefault();
          void store.setVolume(Math.max(0, store.volume - 5));
          break;
        }
        case "KeyM": {
          event.preventDefault();
          if (store.volume > 0) {
            volumeBeforeMute.current = store.volume;
            void store.setVolume(0);
          } else {
            void store.setVolume(volumeBeforeMute.current || 100);
          }
          break;
        }
        case "KeyF": {
          event.preventDefault();
          void ui.toggleFullscreen();
          break;
        }
        case "KeyN": {
          event.preventDefault();
          void store.playNext();
          break;
        }
        case "KeyP": {
          event.preventDefault();
          void store.playPrev();
          break;
        }
        default:
          break;
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
