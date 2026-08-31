import { useCallback, useEffect, useRef, useState } from "react";

const BOTTOM_ZONE_PX = 88;
const HIDE_DELAY_MS = 2800;

/** Fullscreen cinema: reveal bottom bar when the cursor nears the screen bottom. */
export function usePlaybackChromeReveal(enabled: boolean) {
  const [visible, setVisible] = useState(true);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pinned = useRef(false);

  const clearHideTimer = useCallback(() => {
    if (hideTimer.current) {
      clearTimeout(hideTimer.current);
      hideTimer.current = null;
    }
  }, []);

  const scheduleHide = useCallback(() => {
    clearHideTimer();
    if (!enabled || pinned.current) return;
    hideTimer.current = setTimeout(() => {
      setVisible(false);
    }, HIDE_DELAY_MS);
  }, [clearHideTimer, enabled]);

  const reveal = useCallback(() => {
    setVisible(true);
    scheduleHide();
  }, [scheduleHide]);

  const pin = useCallback(() => {
    pinned.current = true;
    clearHideTimer();
    setVisible(true);
  }, [clearHideTimer]);

  const unpin = useCallback(() => {
    pinned.current = false;
    scheduleHide();
  }, [scheduleHide]);

  useEffect(() => {
    if (!enabled) {
      pinned.current = false;
      clearHideTimer();
      setVisible(true);
      return;
    }

    setVisible(false);
    pinned.current = false;

    const onMove = (event: MouseEvent) => {
      if (event.clientY >= window.innerHeight - BOTTOM_ZONE_PX) {
        reveal();
      }
    };

    window.addEventListener("mousemove", onMove);
    return () => {
      window.removeEventListener("mousemove", onMove);
      clearHideTimer();
    };
  }, [clearHideTimer, enabled, reveal]);

  return { visible, reveal, pin, unpin, scheduleHide };
}
