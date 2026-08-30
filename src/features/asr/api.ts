import { Channel, invoke } from "@tauri-apps/api/core";

import type { Transcript } from "@/features/transcript";

import type { AsrEvent, AsrStatus } from "./types";

export function getAsrStatus(): Promise<AsrStatus> {
  return invoke("asr_status");
}

export async function transcribeOnDemand(
  path: string,
  onEvent?: (event: AsrEvent) => void,
): Promise<Transcript> {
  const channel = new Channel<AsrEvent>();
  if (onEvent) {
    channel.onmessage = onEvent;
  }
  return invoke<Transcript>("asr_transcribe", { path, onEvent: channel });
}
