import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  AcpClientSettings,
  AcpEvent,
  AcpModelDiscoveryResult,
  AcpSessionModelOptions,
  AcpStatus,
  AgentSessionListResult,
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

export function listAcpAgentSessions(
  cwd?: string | null,
): Promise<AgentSessionListResult> {
  return invoke<AgentSessionListResult>("acp_list_agent_sessions", {
    cwd: cwd ?? null,
  });
}

export type LoadedTranscriptEvent = {
  role: "user" | "agent" | "tool";
  text: string;
};

/** Pasted image wire input: raw base64 (no `data:` prefix). */
export type PromptImageInput = {
  mimeType: string;
  data: string;
};

export function acpLoadSession(options: {
  sessionId: string;
  cwd?: string | null;
}): Promise<LoadedTranscriptEvent[]> {
  return invoke<LoadedTranscriptEvent[]>("acp_load_session", {
    sessionId: options.sessionId,
    cwd: options.cwd ?? null,
  });
}

export function acpDeleteSession(sessionId: string): Promise<void> {
  return invoke("acp_delete_session", { sessionId });
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

export async function acpSyncMcpCapabilities(options?: {
  cwd?: string;
  clientSettings?: AcpClientSettings;
}): Promise<void> {
  await invoke("acp_sync_mcp_capabilities", {
    cwd: options?.cwd ?? null,
    clientSettings: options?.clientSettings ?? null,
  });
}

export async function acpPrompt(
  text: string,
  onEvent?: (event: AcpEvent) => void,
  options?: {
    cwd?: string;
    profileId?: string;
    context?: VideoPromptContext;
    images?: PromptImageInput[];
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
    images: options?.images ?? null,
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

export async function acpSwitchSession(
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
  await invoke("acp_switch_session", {
    cwd: options?.cwd ?? null,
    profileId: options?.profileId ?? null,
    savedSession: options?.savedSession ?? null,
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

export function acpLoginAntigravity(proxyPort?: number): Promise<string> {
  return invoke<string>("acp_login_antigravity", {
    proxyPort: proxyPort ?? null,
  });
}
