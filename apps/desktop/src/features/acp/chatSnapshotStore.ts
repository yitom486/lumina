import { create } from "zustand";

import type { ChatRestoreSnapshot } from "./chatRestore";

/**
 * 聊天快照内存镜像（B3 SQLite only）。
 *
 * zustand 只存同步内存态 `snapshots[(profile, session)]`，首渲染不阻塞：
 * 挂载瞬间读这里（miss 即空），DB 回填由 hydrate hook 在 effect 里异步写进来。
 * hints 的内存镜像仍在 `useAcpSessionStore.savedSessions[profile]`，
 * 两者合在一起才是完整的“内存镜像”，热路径调用方一律同步读、不得 await。
 *
 * 非持久化：重启恢复靠 SQLite 主层，内存只活当次运行。
 * `discardTransient` 删内存时调这里（不调 DB 删除，见其注释）。
 */

export function chatSnapshotKeyFor(profileId: string, sessionId: string): string {
  return `${profileId}\0${sessionId}`;
}

type ChatSnapshotMirror = {
  snapshots: Record<string, ChatRestoreSnapshot>;
  snapshotFor: (
    profileId: string,
    sessionId: string,
  ) => ChatRestoreSnapshot | null;
  setSnapshot: (
    profileId: string,
    sessionId: string,
    snapshot: ChatRestoreSnapshot,
  ) => void;
  clearSnapshot: (profileId: string, sessionId: string) => void;
  clearProfile: (profileId: string) => void;
  clearAll: () => void;
};

function normalizeKey(profileId: string): string | null {
  const key = profileId.trim();
  return key.length > 0 ? key : null;
}

function normalizeSession(sessionId: string | null | undefined): string {
  if (typeof sessionId !== "string") return "";
  return sessionId;
}

export const useChatSnapshotStore = create<ChatSnapshotMirror>()(
  (set, get) => ({
    snapshots: {},
    snapshotFor: (profileId, sessionId) => {
      const profileKey = normalizeKey(profileId);
      if (!profileKey) return null;
      return (
        get().snapshots[
          chatSnapshotKeyFor(profileKey, normalizeSession(sessionId))
        ] ?? null
      );
    },
    setSnapshot: (profileId, sessionId, snapshot) =>
      set((state) => {
        const profileKey = normalizeKey(profileId);
        if (!profileKey) return state;
        const sessionKey = normalizeSession(sessionId);
        return {
          snapshots: {
            ...state.snapshots,
            [chatSnapshotKeyFor(profileKey, sessionKey)]: {
              ...snapshot,
              profileId: profileKey,
            },
          },
        };
      }),
    clearSnapshot: (profileId, sessionId) =>
      set((state) => {
        const profileKey = normalizeKey(profileId);
        if (!profileKey) return state;
        const key = chatSnapshotKeyFor(
          profileKey,
          normalizeSession(sessionId),
        );
        if (!(key in state.snapshots)) return state;
        const next = { ...state.snapshots };
        delete next[key];
        return { snapshots: next };
      }),
    clearProfile: (profileId) =>
      set((state) => {
        const profileKey = normalizeKey(profileId);
        if (!profileKey) return state;
        const prefix = `${profileKey}\0`;
        const next: Record<string, ChatRestoreSnapshot> = {};
        let changed = false;
        for (const [key, value] of Object.entries(state.snapshots)) {
          if (key === profileKey || key.startsWith(prefix)) {
            changed = true;
            continue;
          }
          next[key] = value;
        }
        return changed ? { snapshots: next } : state;
      }),
    clearAll: () => set({ snapshots: {} }),
  }),
);
