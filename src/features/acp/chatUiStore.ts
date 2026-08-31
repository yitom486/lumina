import { create } from "zustand";
import { persist } from "zustand/middleware";

/** Chat UI-only prefs (not sent to Rust). */
type ChatUiStore = {
  showActivityWhileDone: boolean;
  setShowActivityWhileDone: (value: boolean) => void;
};

export const useChatUiStore = create<ChatUiStore>()(
  persist(
    (set) => ({
      showActivityWhileDone: false,
      setShowActivityWhileDone: (showActivityWhileDone) =>
        set({ showActivityWhileDone }),
    }),
    { name: "lumina-chat-ui" },
  ),
);
