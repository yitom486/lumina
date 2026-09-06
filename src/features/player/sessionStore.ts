/** Last opened media session (localStorage). Separate from live player mirror. */

import { create } from "zustand";
import { persist } from "zustand/middleware";

export type SessionSnapshot = {
  lastPath: string | null;
  lastDirectory: string | null;
  lastPositionMs: number;
  updatedAt: number;
};

type SessionState = SessionSnapshot & {
  saveSession: (args: {
    path: string;
    positionMs: number;
    directory?: string | null;
  }) => void;
  clearSession: () => void;
};

const EMPTY: SessionSnapshot = {
  lastPath: null,
  lastDirectory: null,
  lastPositionMs: 0,
  updatedAt: 0,
};

export function isRemotePath(path: string): boolean {
  const lower = path.trim().toLowerCase();
  return lower.startsWith("https://") || lower.startsWith("http://");
}

export function parentDirectory(path: string): string | null {
  // Remote URLs have no local parent: never let `https:\\host` leak into
  // file dialogs or library roots (it used to pollute both after online play).
  if (isRemotePath(path)) return null;
  const normalized = path.replace(/\//g, "\\").trim();
  const idx = normalized.lastIndexOf("\\");
  if (idx <= 0) return null;
  return normalized.slice(0, idx);
}

export const useSessionStore = create<SessionState>()(
  persist(
    (set) => ({
      ...EMPTY,

      saveSession: ({ path, positionMs, directory }) => {
        const trimmed = path.trim();
        if (!trimmed) return;
        set((state) => ({
          lastPath: trimmed,
          lastDirectory: isRemotePath(trimmed)
            ? state.lastDirectory
            : (directory ?? parentDirectory(trimmed)),
          lastPositionMs: Math.max(0, Math.floor(positionMs)),
          updatedAt: Date.now(),
        }));
      },

      clearSession: () => set({ ...EMPTY }),
    }),
    {
      name: "lumina-session",
      partialize: (state) => ({
        lastPath: state.lastPath,
        lastDirectory: state.lastDirectory,
        lastPositionMs: state.lastPositionMs,
        updatedAt: state.updatedAt,
      }),
    },
  ),
);

/** Prefer session snapshot on cold restore; fall back to per-file progress. */
export function resolveRestorePositionMs(
  path: string,
  session: Pick<SessionSnapshot, "lastPath" | "lastPositionMs">,
  progressPositionMs: number | null | undefined,
): number {
  if (session.lastPath === path && session.lastPositionMs > 0) {
    return session.lastPositionMs;
  }
  return progressPositionMs ?? 0;
}
