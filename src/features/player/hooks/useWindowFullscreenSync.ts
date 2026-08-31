/** Keep React chrome aligned with the authoritative native window state.
 * This is especially important after Vite HMR: the OS window remains
 * fullscreen while Zustand is recreated with its initial state.
 */

import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { useUiStore } from "../uiStore";

export function useWindowFullscreenSync(): void {
  const syncFullscreen = useUiStore((state) => state.syncFullscreen);

  useEffect(() => {
    let disposed = false;
    let unlistenResize: (() => void) | undefined;
    let retryTimer: number | undefined;

    const sync = () => {
      void syncFullscreen();
    };

    const syncAfterResize = () => {
      sync();
      if (retryTimer !== undefined) window.clearTimeout(retryTimer);
      // Some Windows transitions report the resize one frame before the
      // fullscreen flag changes. Re-read once after that transition settles.
      retryTimer = window.setTimeout(sync, 100);
    };

    sync();
    void getCurrentWindow()
      .onResized(syncAfterResize)
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        unlistenResize = unlisten;
      })
      .catch((error) => {
        console.error("window resize subscription failed", error);
      });

    return () => {
      disposed = true;
      if (retryTimer !== undefined) window.clearTimeout(retryTimer);
      unlistenResize?.();
    };
  }, [syncFullscreen]);
}
