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
  /** Right panel width in px, shared by the chat dock and the sidebar panels. */
  dockWidth: number;
  setDockWidth: (value: number) => void;
  openChat: () => void;
  closeChat: () => void;
  toggleChat: () => void;
};

/** Default right panel width; also the double-click reset target. */
export const DEFAULT_DOCK_WIDTH = 380;
/** Drag clamp: narrower hides content, wider starves the video surface. */
export const MIN_DOCK_WIDTH = 280;
export const MAX_DOCK_WIDTH = 600;

export function clampDockWidth(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_DOCK_WIDTH;
  return Math.min(MAX_DOCK_WIDTH, Math.max(MIN_DOCK_WIDTH, Math.round(value)));
}

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
      dockWidth: DEFAULT_DOCK_WIDTH,
      setDockWidth: (value) => set({ dockWidth: clampDockWidth(value) }),
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
        dockWidth: state.dockWidth,
      }),
    },
  ),
);
