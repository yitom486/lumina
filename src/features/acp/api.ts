import { Channel, invoke } from "@tauri-apps/api/core";

import type { AcpEvent, AcpStatus } from "./types";

export function getAcpStatus(): Promise<AcpStatus> {
  return invoke("acp_status");
}

export async function acpPrompt(
  text: string,
  onEvent?: (event: AcpEvent) => void,
  cwd?: string,
): Promise<string> {
  const channel = new Channel<AcpEvent>();
  if (onEvent) {
    channel.onmessage = onEvent;
  }
  return invoke<string>("acp_prompt", { text, cwd: cwd ?? null, onEvent: channel });
}

export function acpCancel(): Promise<void> {
  return invoke("acp_cancel");
}
