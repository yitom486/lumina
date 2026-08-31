import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { SavedSessionHint } from "./types";

type AcpSessionStore = {
  savedSession: SavedSessionHint | null;
  setSavedSession: (session: SavedSessionHint) => void;
  clearSavedSession: () => void;
};

/** Session id hint for ACP resume; chat history is not persisted. */
export const useAcpSessionStore = create<AcpSessionStore>()(
  persist(
    (set) => ({
      savedSession: null,
      setSavedSession: (savedSession) => set({ savedSession }),
      clearSavedSession: () => set({ savedSession: null }),
    }),
    {
      name: "lumina-acp-session",
      partialize: (state) => ({
        savedSession: state.savedSession,
      }),
      merge: (persisted, current) => {
        const saved = (persisted ?? {}) as Partial<
          Pick<AcpSessionStore, "savedSession">
        >;
        return {
          ...current,
          savedSession: saved.savedSession ?? current.savedSession ?? null,
        };
      },
    },
  ),
);
