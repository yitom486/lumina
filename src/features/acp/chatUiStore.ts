import { create } from "zustand";
import { persist } from "zustand/middleware";

/**
 * App-level chat chrome — parallel to sidebar / playback, not nested inside them.
 * Once mounted, the ACP panel stays alive until app exit (hide ≠ unmount).
 */
type ChatUiStore = {
  showActivityWhileDone: boolean;
  setShowActivityWhileDone: (value: boolean) => void;
  /** AcpPanel has been opened at least once this session. */
  chatMounted: boolean;
  /** Floating dock visibility (toggle only; does not unmount). */
  chatOpen: boolean;
  /** Agent is streaming a reply; player hotkeys except Space are suppressed. */
  acpResponding: boolean;
  setAcpResponding: (value: boolean) => void;
  openChat: () => void;
  closeChat: () => void;
  toggleChat: () => void;
};

export const useChatUiStore = create<ChatUiStore>()(
  persist(
    (set, get) => ({
      showActivityWhileDone: false,
      setShowActivityWhileDone: (showActivityWhileDone) =>
        set({ showActivityWhileDone }),
      chatMounted: false,
      chatOpen: false,
      acpResponding: false,
      setAcpResponding: (acpResponding) => set({ acpResponding }),
      openChat: () => set({ chatMounted: true, chatOpen: true }),
      closeChat: () => set({ chatOpen: false }),
      toggleChat: () => {
        const { chatOpen } = get();
        if (!chatOpen) {
          set({ chatMounted: true, chatOpen: true });
          return;
        }
        set({ chatOpen: false });
      },
    }),
    {
      name: "lumina-chat-ui",
      partialize: (state) => ({
        showActivityWhileDone: state.showActivityWhileDone,
      }),
    },
  ),
);
