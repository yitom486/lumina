/** Parent directory of a media path, or undefined when no file is open. */
export function workspaceCwdFromMedia(
  path: string | null | undefined,
): string | undefined {
  if (!path) return undefined;
  // Remote media identifiers are URLs, not filesystem workspaces. Let the
  // backend select Lumina's writable ACP workspace for online playback.
  if (/^https?:\/\//i.test(path.trim())) return undefined;
  const normalized = path.replace(/[/\\]+$/, "");
  const idx = Math.max(
    normalized.lastIndexOf("/"),
    normalized.lastIndexOf("\\"),
  );
  if (idx <= 0) return undefined;
  return normalized.slice(0, idx);
}
