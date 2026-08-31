/** UI chrome (fullscreen cinema mode). Separate from Rust playback authority. */

import { create } from "zustand";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { ensureSurfaceBounds } from "./surfaceBridge";

export type SidebarTab =
  | "playlist"
  | "transcript"
  | "notes"
  | "chapters"
  | "acp";

type UiState = {
  fullscreen: boolean;
  sidebarTab: SidebarTab;
  /** Once user opens the chat tab, keep AcpPanel mounted (hidden when inactive). */
  acpPanelAlive: boolean;
  setSidebarTab: (tab: SidebarTab) => void;
  setFullscreen: (value: boolean) => Promise<void>;
  toggleFullscreen: () => Promise<void>;
};

export const useUiStore = create<UiState>((set, get) => ({
  fullscreen: false,
  sidebarTab: "transcript",
  acpPanelAlive: false,

  setSidebarTab: (sidebarTab) => {
    set({
      sidebarTab,
      ...(sidebarTab === "acp" ? { acpPanelAlive: true } : {}),
    });
    requestAnimationFrame(() => {
      void ensureSurfaceBounds();
    });
  },

  setFullscreen: async (value) => {
    try {
      await getCurrentWindow().setFullscreen(value);
      set({ fullscreen: value });
      // Let layout settle, then re-measure HWND over the expanded video area.
      requestAnimationFrame(() => {
        void ensureSurfaceBounds();
      });
      window.setTimeout(() => {
        void ensureSurfaceBounds();
      }, 80);
    } catch (error) {
      console.error("setFullscreen failed", error);
    }
  },

  toggleFullscreen: async () => {
    await get().setFullscreen(!get().fullscreen);
  },
}));
