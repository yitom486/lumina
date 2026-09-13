import { useEffect } from "react";

import { usePlayerStore } from "@/features/player";

import { shouldSyncLibraryRoots } from "../playbackRoot";
import { useLibrarySettingsStore } from "../settingsStore";

/** Keep media-library roots aligned with the current video folder unless user picked manually. */
export function useLibraryPlaybackRootSync() {
  const currentFile = usePlayerStore((s) => s.currentFile);
  const followPlayback = useLibrarySettingsStore((s) => s.rootsFollowPlayback);
  const patchSettings = useLibrarySettingsStore((s) => s.patchSettings);

  useEffect(() => {
    const roots = useLibrarySettingsStore.getState().roots;
    const nextRoots = shouldSyncLibraryRoots(followPlayback, currentFile, roots);
    if (!nextRoots) return;
    patchSettings({ roots: nextRoots });
  }, [currentFile, followPlayback, patchSettings]);
}
