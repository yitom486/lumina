/** While Agent is replying, only Space (play/pause) may reach player hotkeys. */
export function shouldSuppressPlayerHotkeyDuringAcp(
  code: string,
  acpResponding: boolean,
): boolean {
  if (!acpResponding) return false;
  return code !== "Space";
}
