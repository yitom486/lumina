import { Channel, invoke } from "@tauri-apps/api/core";

import type { Transcript } from "@/features/transcript";

import type { AsrEvent, AsrRange, AsrStatus } from "./types";

export function getAsrStatus(): Promise<AsrStatus> {
  return invoke("asr_status");
}

export async function transcribeOnDemand(
  path: string,
  onEvent?: (event: AsrEvent) => void,
  options?: {
    range?: AsrRange | null;
    modelId?: string | null;
  },
): Promise<Transcript> {
  const channel = new Channel<AsrEvent>();
  if (onEvent) {
    channel.onmessage = onEvent;
  }
  return invoke<Transcript>("asr_transcribe", {
    path,
    range: options?.range ?? null,
    modelId: options?.modelId ?? null,
    onEvent: channel,
  });
}
