/**
 * Shared TanStack Query keys for media inspection (O1).
 *
 * Keys are hand-written in several features; factories keep them identical.
 * Naming convention (frozen): media keys stay camelCase (`mediaInfo`);
 * do NOT introduce new kebab-case variants for the same data.
 */

export function mediaInfoKey(path: string | null | undefined) {
  return ["mediaInfo", path] as const;
}

export function mediaToolStatusKey() {
  return ["media-tool-status"] as const;
}
