/** Parent directory of a media path, or undefined when no file is open. */
export function workspaceCwdFromMedia(
  path: string | null | undefined,
): string | undefined {
  if (!path) return undefined;
  const normalized = path.replace(/[/\\]+$/, "");
  const idx = Math.max(
    normalized.lastIndexOf("/"),
    normalized.lastIndexOf("\\"),
  );
  if (idx <= 0) return undefined;
  return normalized.slice(0, idx);
}
