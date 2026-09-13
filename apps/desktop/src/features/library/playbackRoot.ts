import { parentDirectory } from "@/features/player/sessionStore";

/** Parent folder of the currently opened media file, for library watch roots. */
export function libraryRootFromPlaybackPath(
  currentFile: string | null | undefined,
): string | null {
  if (!currentFile?.trim()) return null;
  return parentDirectory(currentFile.trim());
}

export function shouldSyncLibraryRoots(
  followPlayback: boolean,
  currentFile: string | null | undefined,
  currentRoots: string[],
): string[] | null {
  if (!followPlayback) return null;
  const next = libraryRootFromPlaybackPath(currentFile);
  if (!next) return null;
  if (currentRoots.length === 1 && currentRoots[0] === next) return null;
  return [next];
}
