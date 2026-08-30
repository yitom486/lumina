/** Zustand mirror of Rust Player Runtime. Do not invent Playing/Paused locally. */

import { create } from "zustand";

import { errorMessage } from "@/lib/format";

import * as api from "./api";
import {
  IDLE_SNAPSHOT,
  type PlayerErrorDto,
  type PlayerEvent,
  type PlayerSnapshot,
} from "./types";
import { ensureSurfaceBounds } from "./surfaceBridge";

type PlayerStore = PlayerSnapshot & {
  busy: boolean;
  statusMessage: string;
  isSeeking: boolean;

  applySnapshot: (snapshot: PlayerSnapshot) => void;
  applyEvent: (event: PlayerEvent) => void;
  setSeeking: (seeking: boolean) => void;
  setPreviewTime: (currentTimeMs: number) => void;
  setStatusMessage: (message: string) => void;

  openFile: () => Promise<void>;
  play: () => Promise<void>;
  pause: () => Promise<void>;
  stop: () => Promise<void>;
  seek: (positionMs: number) => Promise<void>;
  setVolume: (volume: number) => Promise<void>;
  setRate: (rate: number) => Promise<void>;
  setSubtitle: (args: {
    source: "Embedded" | "Sidecar" | "None";
    streamIndex?: number | null;
    externalPath?: string | null;
  }) => Promise<void>;
};

function fileName(path: string | null): string | null {
  if (!path) return null;
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

export const usePlayerStore = create<PlayerStore>((set, get) => ({
  ...IDLE_SNAPSHOT,
  busy: false,
  statusMessage: "选择本地视频以验证 native libmpv 画面",
  isSeeking: false,

  applySnapshot: (snapshot) => {
    set({
      status: snapshot.status,
      currentTimeMs: snapshot.currentTimeMs,
      durationMs: snapshot.durationMs,
      volume: snapshot.volume,
      rate: snapshot.rate,
      currentFile: snapshot.currentFile,
      error: snapshot.error,
    });
  },

  applyEvent: (event) => {
    switch (event.type) {
      case "StateChanged":
        set({ status: event.payload.status });
        break;
      case "PositionChanged":
        if (!get().isSeeking) {
          set({ currentTimeMs: event.payload.positionMs });
        }
        break;
      case "DurationChanged":
        set({ durationMs: event.payload.durationMs });
        break;
      case "FileLoaded":
        set({
          currentFile: event.payload.path,
          durationMs: event.payload.durationMs,
          currentTimeMs: 0,
          error: null,
          statusMessage: `Playing: ${fileName(event.payload.path) ?? event.payload.path}`,
        });
        break;
      case "Ended":
        set({ status: "Ended", statusMessage: "Ended" });
        break;
      case "Error":
        set({
          status: "Error",
          error: event.payload.error,
          statusMessage: event.payload.error.message,
        });
        break;
    }
  },

  setSeeking: (seeking) => set({ isSeeking: seeking }),
  setPreviewTime: (currentTimeMs) => set({ currentTimeMs }),
  setStatusMessage: (statusMessage) => set({ statusMessage }),

  openFile: async () => {
    set({ busy: true });
    try {
      const path = await api.pickVideoFile();
      if (!path) {
        return;
      }
      await ensureSurfaceBounds();
      const snapshot = await api.openPlayer(path);
      get().applySnapshot(snapshot);
      set({
        statusMessage: snapshot.currentFile
          ? `Playing: ${fileName(snapshot.currentFile) ?? snapshot.currentFile}`
          : "Opened",
        error: snapshot.error,
      });
    } catch (error) {
      const message = errorMessage(error);
      set({
        status: "Error",
        error: toErrorDto(error),
        statusMessage: message,
      });
    } finally {
      set({ busy: false });
    }
  },

  play: async () => {
    try {
      get().applySnapshot(await api.playPlayer());
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  },

  pause: async () => {
    try {
      get().applySnapshot(await api.pausePlayer());
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  },

  stop: async () => {
    try {
      get().applySnapshot(await api.stopPlayer());
      set({ statusMessage: "Stopped" });
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  },

  seek: async (positionMs) => {
    try {
      get().applySnapshot(await api.seekPlayer(positionMs));
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    } finally {
      set({ isSeeking: false });
    }
  },

  setVolume: async (volume) => {
    try {
      get().applySnapshot(await api.setPlayerVolume(volume));
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  },

  setRate: async (rate) => {
    try {
      get().applySnapshot(await api.setPlayerRate(rate));
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  },

  setSubtitle: async (args) => {
    try {
      get().applySnapshot(await api.setPlayerSubtitle(args));
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  },
}));

function toErrorDto(error: unknown): PlayerErrorDto | null {
  if (typeof error === "object" && error && "code" in error && "message" in error) {
    return {
      code: String((error as { code: string }).code),
      message: String((error as { message: string }).message),
      details:
        "details" in error && (error as { details?: string }).details != null
          ? String((error as { details?: string }).details)
          : undefined,
    };
  }
  return { code: "InternalError", message: errorMessage(error) };
}
