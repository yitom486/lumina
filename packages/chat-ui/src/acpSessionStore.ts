import { create } from "zustand";

import type { SavedSessionHint } from "./types";

type AcpSessionStore = {
  savedSession: SavedSessionHint | null;
  setSavedSession: (session: SavedSessionHint) => void;
  clearSavedSession: () => void;
};

/**
 * 当前连接的原生会话 id。纯内存，不落盘：历史真相在 Agent 侧
 * `session/list`，重启后由用户从历史列表选择，不再静默恢复旧线程。
 */
export const useAcpSessionStore = create<AcpSessionStore>()((set) => ({
  savedSession: null,
  setSavedSession: (savedSession) => set({ savedSession }),
  clearSavedSession: () => set({ savedSession: null }),
}));
