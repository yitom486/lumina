import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { SavedSessionHint } from "./types";

export type SavedSessionsByProfile = Record<string, SavedSessionHint>;

type AcpSessionStore = {
  /**
   * 按 agent profile 分键的原生会话 hint。
   *
   * 多智能体隔离的不变量：codex 的续聊 hint 只活在 `"codex"` 键下，
   * claude/cursor 各有各的键。切 profile 是“换世界”，绝不删别的世界的记忆；
   * 切回来拿自家键 resume 即可。调用方读写必须显式带 profileId，
   * 无参的全局单槽是已下线的形态（见 v0 迁移）。
   */
  savedSessions: SavedSessionsByProfile;
  savedSessionFor: (profileId: string) => SavedSessionHint | null;
  setSavedSessionFor: (profileId: string, session: SavedSessionHint) => void;
  clearSavedSessionFor: (profileId: string) => void;
  clearAllSavedSessions: () => void;
};

function normalizeProfileKey(profileId: string): string | null {
  const key = profileId.trim();
  return key.length > 0 ? key : null;
}

function isValidHint(value: unknown): value is SavedSessionHint {
  if (typeof value !== "object" || value === null) return false;
  const hint = value as Record<string, unknown>;
  return (
    typeof hint.sessionId === "string" &&
    hint.sessionId.length > 0 &&
    typeof hint.profileId === "string" &&
    hint.profileId.trim().length > 0 &&
    typeof hint.cwd === "string"
  );
}

function normalizeSessions(value: unknown): SavedSessionsByProfile {
  if (typeof value !== "object" || value === null) return {};
  const next: SavedSessionsByProfile = {};
  for (const [key, hint] of Object.entries(value as Record<string, unknown>)) {
    const profileKey = normalizeProfileKey(key);
    if (!profileKey || !isValidHint(hint)) continue;
    // 键即真相：hint 里的 profileId 强制与键一致，调用方传错也串不了台。
    next[profileKey] = { ...hint, profileId: profileKey };
  }
  return next;
}

type PersistedV0 = { savedSession?: unknown };
type PersistedV1 = { savedSessions?: unknown };

function migratePersisted(persisted: unknown): {
  savedSessions: SavedSessionsByProfile;
} {
  const record = (persisted ?? {}) as PersistedV0 & PersistedV1;
  const migrated = normalizeSessions(record.savedSessions);
  // v0 单槽：按 hint 自带的 profileId 落键；无合法 hint 就空 map。
  if (Object.keys(migrated).length === 0 && isValidHint(record.savedSession)) {
    const key = normalizeProfileKey(record.savedSession.profileId);
    if (key) migrated[key] = { ...record.savedSession, profileId: key };
  }
  return { savedSessions: migrated };
}

/**
 * 各 agent 的原生会话 id。落盘持久化：重启后 connect 拿自家键的 hint 当
 * resume hint 传给后端（真续聊第 2 层）；作用域校验（profile+cwd）在后端
 * 做（scope_mismatch 直接建新，绝不跨 profile 续别人的线程）。
 */
export const useAcpSessionStore = create<AcpSessionStore>()(
  persist(
    (set, get) => ({
      savedSessions: {},
      savedSessionFor: (profileId) => {
        const key = normalizeProfileKey(profileId);
        return key ? (get().savedSessions[key] ?? null) : null;
      },
      setSavedSessionFor: (profileId, session) =>
        set((state) => {
          const key = normalizeProfileKey(profileId);
          if (!key) return state;
          return {
            savedSessions: {
              ...state.savedSessions,
              [key]: { ...session, profileId: key },
            },
          };
        }),
      clearSavedSessionFor: (profileId) =>
        set((state) => {
          const key = normalizeProfileKey(profileId);
          if (!key || !(key in state.savedSessions)) return state;
          const next = { ...state.savedSessions };
          delete next[key];
          return { savedSessions: next };
        }),
      clearAllSavedSessions: () => set({ savedSessions: {} }),
    }),
    {
      name: "lumina-acp-session",
      version: 1,
      partialize: (state) => ({ savedSessions: state.savedSessions }),
      // v0→v1 必须走 migrate：光靠 merge 不够，version 不一致时 zustand
      // 会直接丢弃落盘（见 warning "couldn't be migrated"）。migrate 与
      // merge 都是幂等的，调两次也不怕。
      migrate: (persistedState) => migratePersisted(persistedState),
      merge: (persisted, current) => ({
        ...current,
        ...migratePersisted(persisted),
      }),
    },
  ),
);
