import { Channel, invoke } from "@tauri-apps/api/core";

import type { AcpEvent, AcpStatus, AgentProfileInput } from "./types";

export function getAcpStatus(): Promise<AcpStatus> {
  return invoke("acp_status");
}

export function setActiveAcpProfile(id: string): Promise<AcpStatus> {
  return invoke("acp_set_active_profile", { id });
}

export function upsertAcpProfile(profile: AgentProfileInput): Promise<unknown> {
  return invoke("acp_upsert_profile", { profile });
}

export async function acpPrompt(
  text: string,
  onEvent?: (event: AcpEvent) => void,
  options?: { cwd?: string; profileId?: string },
): Promise<string> {
  const channel = new Channel<AcpEvent>();
  if (onEvent) {
    channel.onmessage = onEvent;
  }
  return invoke<string>("acp_prompt", {
    text,
    cwd: options?.cwd ?? null,
    profileId: options?.profileId ?? null,
    onEvent: channel,
  });
}

export function acpCancel(): Promise<void> {
  return invoke("acp_cancel");
}
