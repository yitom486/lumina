import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

/** Crash/log export entry point. Backend never fails (temp fallback). */
export function getLogDir(): Promise<string> {
  return invoke<string>("system_log_dir");
}

/** Reveal the file-log directory in the OS file manager. */
export async function revealLogDir(): Promise<void> {
  const dir = await getLogDir();
  await revealItemInDir(dir);
}
