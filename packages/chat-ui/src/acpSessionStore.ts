import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { SavedSessionHint } from "./types";

type AcpSessionStore = {
  savedSession: SavedSessionHint | null;
  setSavedSession: (session: SavedSessionHint) => void;
  clearSavedSession: () => void;
};

/**
 * 当前连接的原生会话 id。落盘持久化：重启后 connect 拿它当 resume hint
 * 传给后端（真续聊第 2 层）；作用域校验（profile+cwd）在调用方做，
 * 对不上就当没有、不静默续别人的线程。
 */
export const useAcpSessionStore = create<AcpSessionStore>()(
  persist(
    (set) => ({
      savedSession: null,
      setSavedSession: (savedSession) => set({ savedSession }),
      clearSavedSession: () => set({ savedSession: null }),
    }),
    {
      name: "lumina-acp-session",
      partialize: (state) => ({ savedSession: state.savedSession }),
    },
  ),
);
