import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  AcpClientSettings,
  AcpEvent,
  AcpModelDiscoveryResult,
  AcpSessionModelOptions,
  AcpStatus,
  AgentProfilesHint,
  SavedSessionHint,
  VideoPromptContext,
} from "./types";

export function getAcpStatus(profiles: AgentProfilesHint): Promise<AcpStatus> {
  return invoke("acp_status", { profiles });
}

export async function acpConnect(
  onEvent?: (event: AcpEvent) => void,
  options?: {
    cwd?: string;
    profileId?: string;
    savedSession?: SavedSessionHint | null;
    clientSettings?: AcpClientSettings;
    profiles: AgentProfilesHint;
  },
): Promise<void> {
  const channel = new Channel<AcpEvent>();
  if (onEvent) {
    channel.onmessage = onEvent;
  }
  await invoke("acp_connect", {
    cwd: options?.cwd ?? null,
    profileId: options?.profileId ?? null,
    savedSession: options?.savedSession ?? null,
    clientSettings: options?.clientSettings ?? null,
    profiles: options?.profiles,
    onEvent: channel,
  });
}

export function respondAcpPermission(
  requestId: string,
  optionId: string | null,
): Promise<void> {
  return invoke("acp_respond_permission", {
    requestId,
    optionId,
  });
}

export async function acpPrompt(
  text: string,
  onEvent?: (event: AcpEvent) => void,
  options?: {
    cwd?: string;
    profileId?: string;
    context?: VideoPromptContext;
    historyContext?: string | null;
    savedSession?: SavedSessionHint | null;
    clientSettings?: AcpClientSettings;
    profiles: AgentProfilesHint;
  },
): Promise<string> {
  const channel = new Channel<AcpEvent>();
  if (onEvent) {
    channel.onmessage = onEvent;
  }
  return invoke<string>("acp_prompt", {
    text,
    cwd: options?.cwd ?? null,
    profileId: options?.profileId ?? null,
    context: options?.context ?? null,
    historyContext: options?.historyContext ?? null,
    savedSession: options?.savedSession ?? null,
    clientSettings: options?.clientSettings ?? null,
    profiles: options?.profiles,
    onEvent: channel,
  });
}

export function acpCancel(): Promise<void> {
  return invoke("acp_cancel");
}

export function acpClose(): Promise<void> {
  return invoke("acp_close");
}

export async function acpNewChat(
  onEvent?: (event: AcpEvent) => void,
  options?: {
    cwd?: string;
    profileId?: string;
    clientSettings?: AcpClientSettings;
    profiles: AgentProfilesHint;
  },
): Promise<void> {
  const channel = new Channel<AcpEvent>();
  if (onEvent) {
    channel.onmessage = onEvent;
  }
  await invoke("acp_new_chat", {
    cwd: options?.cwd ?? null,
    profileId: options?.profileId ?? null,
    clientSettings: options?.clientSettings ?? null,
    profiles: options?.profiles,
    onEvent: channel,
  });
}

export async function acpSetSessionModel(
  options: {
    modelId?: string | null;
    reasoningEffort?: string | null;
  },
  onEvent?: (event: AcpEvent) => void,
): Promise<AcpSessionModelOptions> {
  const channel = new Channel<AcpEvent>();
  if (onEvent) {
    channel.onmessage = onEvent;
  }
  return invoke<AcpSessionModelOptions>("acp_set_session_model", {
    modelId: options.modelId ?? null,
    reasoningEffort: options.reasoningEffort ?? null,
    onEvent: channel,
  });
}

export function discoverAcpModels(
  profiles: AgentProfilesHint,
  profileId: string,
): Promise<AcpModelDiscoveryResult> {
  return invoke<AcpModelDiscoveryResult>("library_agent_models_discover", {
    config: { profileId, profiles },
  });
}
