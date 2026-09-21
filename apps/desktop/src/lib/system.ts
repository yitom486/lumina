import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

export type SystemStartupNotice = {
  code: string;
  message: string;
  canRetry: boolean;
};

/** Crash/log export entry point. Backend never fails (temp fallback). */
export function getLogDir(): Promise<string> {
  return invoke<string>("system_log_dir");
}

/** Read the safe business notice for the previous unclean native exit. */
export function getStartupNotice(): Promise<SystemStartupNotice | null> {
  return invoke<SystemStartupNotice | null>("system_startup_notice");
}

/** Reveal the file-log directory in the OS file manager. */
export async function revealLogDir(): Promise<void> {
  const dir = await getLogDir();
  await revealItemInDir(dir);
}
