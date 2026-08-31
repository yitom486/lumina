/** Zustand mirror of Rust Player Runtime. Do not invent Playing/Paused locally. */

import { create } from "zustand";

import { errorMessage } from "@/lib/format";

import * as api from "./api";
import {
  shouldOfferResume,
  useProgressStore,
} from "./progressStore";
import {
  IDLE_SNAPSHOT,
  type PlayerErrorDto,
  type PlayerEvent,
  type PlayerSnapshot,
} from "./types";
import { ensureSurfaceBounds } from "./surfaceBridge";
import { useUiStore } from "./uiStore";

export type ResumeToast = {
  path: string;
  positionMs: number;
};

type PlayerStore = PlayerSnapshot & {
  /** Rust is authoritative; false means the WebView must not hide/show HWND yet. */
  runtimeSynced: boolean;
  busy: boolean;
  statusMessage: string;
  isSeeking: boolean;
  /** Non-blocking MX-style chip; auto-resume already applied. */
  resumeToast: ResumeToast | null;
  /** Skip auto-resume once after handled for this open. */
  resumeHandledForPath: string | null;
  /** Same-folder videos (sorted). */
  playlist: string[];
  playlistIndex: number;

  applySnapshot: (snapshot: PlayerSnapshot) => void;
  applyEvent: (event: PlayerEvent) => void;
  setRuntimeSynced: (runtimeSynced: boolean) => void;
  setSeeking: (seeking: boolean) => void;
  setPreviewTime: (currentTimeMs: number) => void;
  setStatusMessage: (message: string) => void;
  dismissResumeToast: () => void;
  restartFromBeginning: () => Promise<void>;

  openFile: () => Promise<void>;
  openPath: (path: string, options?: { rebuildPlaylist?: boolean }) => Promise<void>;
  /** Rebuild same-folder playlist when UI resyncs but list was lost (HMR / remount). */
  syncPlaylistForPath: (path: string) => Promise<void>;
  playNext: () => Promise<void>;
  playPrev: () => Promise<void>;
  play: () => Promise<void>;
  pause: () => Promise<void>;
  stop: () => Promise<void>;
  togglePlayPause: () => Promise<void>;
  seek: (positionMs: number) => Promise<void>;
  setVolume: (volume: number) => Promise<void>;
  setRate: (rate: number) => Promise<void>;
  setSubtitle: (args: {
    source: "Embedded" | "Sidecar" | "None";
    streamIndex?: number | null;
    externalPath?: string | null;
  }) => Promise<void>;
  setAudio: (streamIndex: number) => Promise<void>;
};

function fileName(path: string | null): string | null {
  if (!path) return null;
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

function indexOfPath(playlist: string[], path: string): number {
  const needle = path.replace(/\//g, "\\").toLowerCase();
  return playlist.findIndex(
    (p) => p.replace(/\//g, "\\").toLowerCase() === needle,
  );
}

/** MX Player style: seek+play immediately; show dismissible toast (no modal). */
function maybeResume(
  path: string,
  durationMs: number,
  get: () => PlayerStore,
  set: (
    partial:
      | Partial<PlayerStore>
      | ((state: PlayerStore) => Partial<PlayerStore>),
  ) => void,
) {
  if (get().resumeHandledForPath === path) return;
  const saved = useProgressStore.getState().getProgress(path);
  if (!saved || !shouldOfferResume(saved.positionMs, durationMs)) {
    set({ resumeHandledForPath: path, resumeToast: null });
    return;
  }

  const positionMs = saved.positionMs;
  set({
    resumeHandledForPath: path,
    resumeToast: { path, positionMs },
  });

  void (async () => {
    try {
      await get().seek(positionMs);
      // open() already leaves the backend in Playing — only play if not.
      if (get().status !== "Playing") {
        await get().play();
      }
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  })();
}

export const usePlayerStore = create<PlayerStore>((set, get) => ({
  ...IDLE_SNAPSHOT,
  runtimeSynced: false,
  busy: false,
  statusMessage: "打开本地视频开始观看",
  isSeeking: false,
  resumeToast: null,
  resumeHandledForPath: null,
  playlist: [],
  playlistIndex: -1,

  applySnapshot: (snapshot) => {
    set((state) => ({
      status: snapshot.status,
      currentTimeMs: snapshot.currentTimeMs,
      // Never clobber a known duration with 0 (mpv often reports 0 until demux settles;
      // a later seek/play snapshot would otherwise wipe DurationChanged).
      // Exception: resync with a real file + positive duration from Rust always wins.
      durationMs:
        snapshot.durationMs > 0
          ? snapshot.durationMs
          : snapshot.currentFile
            ? state.durationMs
            : 0,
      volume: snapshot.volume,
      rate: snapshot.rate,
      currentFile: snapshot.currentFile,
      error: snapshot.error,
    }));
  },

  applyEvent: (event) => {
    switch (event.type) {
      case "StateChanged":
        set({ status: event.payload.status });
        break;
      case "PositionChanged":
        // Ignore ghost ticks after HMR reset (Zustand idle, mpv still ticking).
        if (!get().currentFile) break;
        if (!get().isSeeking) {
          set({ currentTimeMs: event.payload.positionMs });
        }
        break;
      case "DurationChanged":
        set({ durationMs: event.payload.durationMs });
        break;
      case "FileLoaded": {
        const path = event.payload.path;
        const durationMs = event.payload.durationMs;
        const idx = indexOfPath(get().playlist, path);
        set({
          currentFile: path,
          durationMs,
          currentTimeMs: 0,
          error: null,
          statusMessage: fileName(path) ?? path,
          playlistIndex: idx >= 0 ? idx : get().playlistIndex,
        });
        if (get().playlist.length === 0 || idx < 0) {
          void get().syncPlaylistForPath(path);
        }
        maybeResume(path, durationMs, get, set);
        break;
      }
      case "Ended":
        set({ status: "Ended", statusMessage: "播放结束" });
        // Auto-advance folder playlist when possible.
        void get().playNext();
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

  setRuntimeSynced: (runtimeSynced) => set({ runtimeSynced }),

  setSeeking: (seeking) => set({ isSeeking: seeking }),
  setPreviewTime: (currentTimeMs) => set({ currentTimeMs }),
  setStatusMessage: (statusMessage) => set({ statusMessage }),

  dismissResumeToast: () => set({ resumeToast: null }),

  restartFromBeginning: async () => {
    const toast = get().resumeToast;
    set({ resumeToast: null });
    const path = toast?.path ?? get().currentFile;
    if (path) {
      useProgressStore.getState().clearProgress(path);
    }
    try {
      await get().seek(0);
      await get().play();
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  },

  openFile: async () => {
    set({ busy: true });
    try {
      const path = await api.pickVideoFile();
      if (!path) return;
      await get().openPath(path, { rebuildPlaylist: true });
      useUiStore.getState().setSidebarTab("playlist");
    } finally {
      set({ busy: false });
    }
  },

  syncPlaylistForPath: async (path) => {
    const idx = indexOfPath(get().playlist, path);
    if (get().playlist.length > 0 && idx >= 0) {
      if (get().playlistIndex !== idx) {
        set({ playlistIndex: idx });
      }
      return;
    }

    try {
      const siblings = await api.listSiblingVideos(path);
      const nextIdx = indexOfPath(siblings, path);
      set({
        playlist: siblings.length > 0 ? siblings : [path],
        playlistIndex: nextIdx >= 0 ? nextIdx : 0,
      });
    } catch (error) {
      console.error("list siblings failed", error);
      set({ playlist: [path], playlistIndex: 0 });
    }
  },

  openPath: async (path, options) => {
    const rebuild = options?.rebuildPlaylist ?? false;
    set({
      resumeHandledForPath: null,
      resumeToast: null,
    });

    if (rebuild) {
      await get().syncPlaylistForPath(path);
    } else {
      const idx = indexOfPath(get().playlist, path);
      if (idx >= 0) {
        set({ playlistIndex: idx });
      } else if (get().playlist.length === 0) {
        await get().syncPlaylistForPath(path);
      }
    }

    try {
      const snapshot = await api.openPlayer(path);
      get().applySnapshot(snapshot);
      set({
        statusMessage: snapshot.currentFile
          ? (fileName(snapshot.currentFile) ?? snapshot.currentFile)
          : "已打开",
        error: snapshot.error,
      });
      if (snapshot.currentFile) {
        maybeResume(
          snapshot.currentFile,
          snapshot.durationMs,
          get,
          set,
        );
      }
      await ensureSurfaceBounds();
    } catch (error) {
      const message = errorMessage(error);
      set({
        status: "Error",
        error: toErrorDto(error),
        statusMessage: message,
      });
    }
  },

  playNext: async () => {
    const { playlist, playlistIndex } = get();
    if (playlist.length === 0) return;
    const next = playlistIndex + 1;
    if (next >= playlist.length) return;
    set({ busy: true });
    try {
      await get().openPath(playlist[next], { rebuildPlaylist: false });
      // Auto-resume already plays; otherwise start playback.
      if (!get().resumeToast) {
        await get().play();
      }
    } finally {
      set({ busy: false });
    }
  },

  playPrev: async () => {
    const { playlist, playlistIndex } = get();
    if (playlist.length === 0) return;
    const prev = playlistIndex - 1;
    if (prev < 0) return;
    set({ busy: true });
    try {
      await get().openPath(playlist[prev], { rebuildPlaylist: false });
      if (!get().resumeToast) {
        await get().play();
      }
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

  togglePlayPause: async () => {
    const { status } = get();
    if (status === "Playing") {
      await get().pause();
    } else {
      await get().play();
    }
  },

  stop: async () => {
    try {
      get().applySnapshot(await api.stopPlayer());
      set({ statusMessage: "已停止" });
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
    const clamped = Math.max(0, Math.min(100, volume));
    set({ volume: clamped });
    try {
      get().applySnapshot(await api.setPlayerVolume(clamped));
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
      console.error("set subtitle failed", error);
    }
  },

  setAudio: async (streamIndex) => {
    try {
      get().applySnapshot(await api.setPlayerAudio(streamIndex));
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
