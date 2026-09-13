/** Zustand mirror of Rust Player Runtime. Do not invent Playing/Paused locally. */

import { create } from "zustand";

import { errorMessage } from "@/lib/format";

import * as api from "./api";
import {
  shouldOfferResume,
  shouldRestoreSessionPosition,
  useProgressStore,
} from "./progressStore";
import { planResumeToast, seekForResume } from "./resumePlayback";
import {
  isRemotePath,
  resolveRestorePositionMs,
  useSessionStore,
} from "./sessionStore";
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
  /** Resume deferred until duration is known (remote / early demux). */
  pendingResumePath: string | null;
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
  openUrl: (url: string) => Promise<void>;
  openPath: (
    path: string,
    options?: { rebuildPlaylist?: boolean; restorePaused?: boolean },
  ) => Promise<boolean>;
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
  setPlaybackFormat: (formatId: string) => Promise<void>;
  setSubtitle: (args: {
    source: "Embedded" | "Sidecar" | "None";
    streamIndex?: number | null;
    externalPath?: string | null;
  }) => Promise<void>;
  setAudio: (streamIndex: number) => Promise<void>;
};

function fileName(path: string | null): string | null {
  if (!path) return null;
  if (isRemotePath(path)) {
    try {
      const host = new URL(path).hostname;
      return host || path;
    } catch {
      return path;
    }
  }
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

function indexOfPath(playlist: string[], path: string): number {
  const needle = path.replace(/\//g, "\\").toLowerCase();
  return playlist.findIndex(
    (p) => p.replace(/\//g, "\\").toLowerCase() === needle,
  );
}

/** MX Player style: seek first, show chip only when position actually resumed. */
function maybeResume(
  path: string,
  /** Must be mpv demux duration — never yt-dlp hint alone. */
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
  if (!saved || !shouldOfferResume(saved.positionMs, durationMs || Number.POSITIVE_INFINITY)) {
    if (saved && durationMs <= 0 && shouldOfferResume(saved.positionMs, Number.POSITIVE_INFINITY)) {
      set({ pendingResumePath: path, resumeToast: null });
      return;
    }
    set({ resumeHandledForPath: path, pendingResumePath: null, resumeToast: null });
    return;
  }

  // Remote CDN demux lags; never seek until mpv reports a real duration.
  if (durationMs <= 0) {
    set({ pendingResumePath: path, resumeToast: null });
    return;
  }

  set({ resumeHandledForPath: path, pendingResumePath: null });

  const targetMs = saved.positionMs;

  void (async () => {
    try {
      const actualMs = await seekForResume(targetMs, async (positionMs) => {
        const snapshot = await api.seekPlayer(positionMs);
        get().applySnapshot(snapshot);
        return snapshot;
      });
      if (actualMs == null) {
        // Stale progress (e.g. ghost position from another file) — drop it.
        useProgressStore.getState().clearProgress(path);
        set({ resumeToast: null });
        return;
      }
      const resolvedDurationMs = get().durationMs || durationMs;
      const planned = planResumeToast(
        targetMs,
        resolvedDurationMs,
        actualMs,
      );
      if (planned.kind === "toast") {
        set({ resumeToast: { path, positionMs: planned.positionMs } });
        if (get().status !== "Playing") {
          await get().play();
        }
      } else {
        set({ resumeToast: null });
      }
    } catch (error) {
      useProgressStore.getState().clearProgress(path);
      set({ resumeToast: null, statusMessage: errorMessage(error) });
    }
  })();
}

async function restorePausedPosition(
  path: string,
  durationMs: number,
  get: () => PlayerStore,
  set: (
    partial:
      | Partial<PlayerStore>
      | ((state: PlayerStore) => Partial<PlayerStore>),
  ) => void,
) {
  if (get().resumeHandledForPath === path) {
    try {
      await get().pause();
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
    return;
  }

  const session = useSessionStore.getState();
  const saved = useProgressStore.getState().getProgress(path);
  const positionMs = resolveRestorePositionMs(
    path,
    session,
    saved?.positionMs,
  );

  set({ resumeHandledForPath: path, resumeToast: null });

  // Cold demux (duration still 0): do not spam seek — stay at start, paused.
  if (durationMs > 0 && shouldRestoreSessionPosition(positionMs, durationMs)) {
    try {
      await seekForResume(positionMs, async (targetMs) => {
        const snapshot = await api.seekPlayer(targetMs);
        get().applySnapshot(snapshot);
        return snapshot;
      });
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    }
  }

  try {
    await get().pause();
  } catch (error) {
    set({ statusMessage: errorMessage(error) });
  }
}

export const usePlayerStore = create<PlayerStore>((set, get) => ({
  ...IDLE_SNAPSHOT,
  runtimeSynced: false,
  busy: false,
  statusMessage: "打开本地视频开始观看",
  isSeeking: false,
  resumeToast: null,
  resumeHandledForPath: null,
  pendingResumePath: null,
  playlist: [],
  playlistIndex: -1,

  applySnapshot: (snapshot) => {
    set((state) => {
      const fileChanged =
        (snapshot.currentFile ?? null) !== (state.currentFile ?? null);
      return {
        status: snapshot.status,
        currentTimeMs: snapshot.currentTimeMs,
        // Same file: never clobber a known duration with 0 (mpv demux lag).
        // New file: always take snapshot duration (even 0) — never keep the previous video's length.
        durationMs: fileChanged
          ? snapshot.durationMs
          : snapshot.durationMs > 0
            ? snapshot.durationMs
            : snapshot.currentFile
              ? state.durationMs
              : 0,
        volume: snapshot.volume,
        rate: snapshot.rate,
        currentFile: snapshot.currentFile,
        mediaId: snapshot.mediaId ?? null,
        sourceKind: snapshot.sourceKind ?? null,
        playbackFormatId: snapshot.playbackFormatId ?? null,
        durationHintMs: snapshot.durationHintMs ?? null,
        error: snapshot.error,
        ...(fileChanged
          ? { pendingResumePath: null, resumeHandledForPath: null }
          : {}),
      };
    });
  },

  applyEvent: (event) => {
    switch (event.type) {
      case "StateChanged":
        set({ status: event.payload.status });
        break;
      case "PositionChanged":
        // Ignore ghost ticks after HMR reset (Zustand idle, mpv still ticking).
        if (!get().currentFile) break;
        // Remote streams often report 0 duration until HLS/DASH settles; ignore
        // stale position ticks so we don't persist another file's offset onto this URL.
        if (get().sourceKind === "remote" && get().durationMs <= 0) break;
        if (!get().isSeeking) {
          set({ currentTimeMs: event.payload.positionMs });
        }
        break;
      case "DurationChanged": {
        const durationMs = event.payload.durationMs;
        set({ durationMs });
        const pending = get().pendingResumePath;
        const path = get().currentFile;
        if (pending && path && pending === path && durationMs > 0) {
          maybeResume(path, durationMs, get, set);
        }
        break;
      }
      case "FileLoaded": {
        const path = event.payload.path;
        const durationMs = event.payload.durationMs;
        const idx = indexOfPath(get().playlist, path);
        set((state) => ({
          currentFile: path,
          durationMs:
            durationMs > 0 ? durationMs : state.durationMs,
          error: null,
          statusMessage: fileName(path) ?? path,
          playlistIndex: idx >= 0 ? idx : state.playlistIndex,
        }));
        if (get().playlist.length === 0 || idx < 0) {
          void get().syncPlaylistForPath(path);
        }
        if (durationMs > 0) {
          const pending = get().pendingResumePath;
          if (pending && pending === path) {
            maybeResume(path, durationMs, get, set);
          }
        }
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
      const path = await api.pickVideoFile(
        useSessionStore.getState().lastDirectory,
      );
      if (!path) return;
      await get().openPath(path, { rebuildPlaylist: true });
      useUiStore.getState().setSidebarTab("playlist");
    } finally {
      set({ busy: false });
    }
  },

  openUrl: async (url) => {
    const trimmed = url.trim();
    if (!trimmed) return;
    set({ busy: true, error: null });
    try {
      const ok = await get().openPath(trimmed, { rebuildPlaylist: true });
      // Stay on「在线」so install/cookie errors remain visible; do not jump to 列表.
      if (ok) {
        useUiStore.getState().setSidebarTab("online");
      }
    } finally {
      set({ busy: false });
    }
  },

  syncPlaylistForPath: async (path) => {
    if (isRemotePath(path)) {
      set({ playlist: [path], playlistIndex: 0 });
      return;
    }

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
    const restorePaused = options?.restorePaused ?? false;
    set({
      resumeHandledForPath: null,
      pendingResumePath: null,
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
        useSessionStore.getState().saveSession({
          path: snapshot.currentFile,
          positionMs: get().currentTimeMs,
        });
        const remote =
          snapshot.sourceKind === "remote" ||
          isRemotePath(snapshot.currentFile);
        if (remote) {
          // Stale localStorage progress + yt-dlp duration hint caused seek storms
          // (Raw(-12)) before CDN demux. Skip auto-resume for remote; play from start.
          useProgressStore.getState().clearProgress(snapshot.currentFile);
          set({
            resumeHandledForPath: snapshot.currentFile,
            pendingResumePath: null,
            resumeToast: null,
          });
          if (restorePaused) {
            try {
              await get().pause();
            } catch (error) {
              set({ statusMessage: errorMessage(error) });
            }
          }
        } else if (restorePaused) {
          await restorePausedPosition(
            snapshot.currentFile,
            get().durationMs || snapshot.durationMs,
            get,
            set,
          );
        } else {
          maybeResume(
            snapshot.currentFile,
            get().durationMs || snapshot.durationMs,
            get,
            set,
          );
        }
      }
      await ensureSurfaceBounds();
      return true;
    } catch (error) {
      const message = errorMessage(error);
      set({
        status: "Error",
        error: toErrorDto(error),
        statusMessage: message,
      });
      return false;
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

  setPlaybackFormat: async (formatId) => {
    set({ busy: true });
    try {
      const snapshot = await api.setPlaybackFormat(formatId);
      get().applySnapshot(snapshot);
      set({ statusMessage: "已切换清晰度" });
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
    } finally {
      set({ busy: false });
    }
  },

  setSubtitle: async (args) => {
    try {
      get().applySnapshot(await api.setPlayerSubtitle(args));
    } catch (error) {
      set({ statusMessage: errorMessage(error) });
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
