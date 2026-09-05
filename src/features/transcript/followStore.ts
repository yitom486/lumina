import { create } from "zustand";
import { persist } from "zustand/middleware";

/**
 * P6-M1 controllable follow. Playback-follow is independent from the prompt
 * anchor (frozen at keystroke); this store only answers "where are the eyes".
 */
export type FollowMode = "following" | "browsing" | "off";

type FollowStore = {
  /** Persisted preference. Default on (matches long-standing auto-follow). */
  followEnabled: boolean;
  /** Ephemeral: user is manually browsing (wheel / touch / keys / select). */
  browsing: boolean;
  setFollowEnabled: (enabled: boolean) => void;
  setBrowsing: (browsing: boolean) => void;
  /** Back-to-position: resume following the active cue. */
  resume: () => void;
  /** Media switched: drop transient browsing state. */
  resetForMedia: () => void;
};

export const useFollowStore = create<FollowStore>()(
  persist(
    (set) => ({
      followEnabled: true,
      browsing: false,
      setFollowEnabled: (followEnabled) =>
        set(
          followEnabled
            ? { followEnabled, browsing: false }
            : { followEnabled },
        ),
      setBrowsing: (browsing) => set({ browsing }),
      resume: () => set({ browsing: false }),
      resetForMedia: () => set({ browsing: false }),
    }),
    {
      name: "lumina-transcript-follow",
      partialize: (state) => ({ followEnabled: state.followEnabled }),
    },
  ),
);

/** Effective mode for the panel. */
export function followMode(
  followEnabled: boolean,
  browsing: boolean,
): FollowMode {
  if (!followEnabled) return "off";
  return browsing ? "browsing" : "following";
}
