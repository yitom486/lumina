/** UI chrome (fullscreen cinema mode). Separate from Rust playback authority. */

import { create } from "zustand";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { ensureSurfaceBounds } from "./surfaceBridge";

export type SidebarTab =
  | "playlist"
  | "transcript"
  | "notes"
  | "chapters"
  | "library";

type UiState = {
  fullscreen: boolean;
  sidebarTab: SidebarTab;
  setSidebarTab: (tab: SidebarTab) => void;
  syncFullscreen: () => Promise<void>;
  setFullscreen: (value: boolean) => Promise<void>;
  toggleFullscreen: () => Promise<void>;
};

export const useUiStore = create<UiState>((set, get) => {
  let fullscreenSyncVersion = 0;

  const refreshSurfaceBounds = () => {
    if (typeof window === "undefined") return;
    // OS fullscreen + React reflow (hide header/sidebar) settle after paint.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        void ensureSurfaceBounds();
      });
    });
    for (const ms of [80, 200, 450]) {
      window.setTimeout(() => {
        void ensureSurfaceBounds();
      }, ms);
    }
  };

  return {
    fullscreen: false,
    sidebarTab: "transcript",

    setSidebarTab: (sidebarTab) => {
      set({ sidebarTab });
      requestAnimationFrame(() => {
        void ensureSurfaceBounds();
      });
    },

    // The native window is authoritative. This fixes HMR / external window
    // changes where a fresh React store would otherwise start at `false`.
    syncFullscreen: async () => {
      const version = ++fullscreenSyncVersion;
      try {
        const fullscreen = await getCurrentWindow().isFullscreen();
        if (version !== fullscreenSyncVersion) return;
        set({ fullscreen });
        refreshSurfaceBounds();
      } catch (error) {
        console.error("fullscreen state sync failed", error);
      }
    },

    setFullscreen: async (value) => {
      try {
        await getCurrentWindow().setFullscreen(value);
        await get().syncFullscreen();
      } catch (error) {
        console.error("setFullscreen failed", error);
      }
    },

    toggleFullscreen: async () => {
      await get().setFullscreen(!get().fullscreen);
    },
  };
});
