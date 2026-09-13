import type {
  AgentModelDiscoveryResult,
  AcpSessionOption,
} from "./types";

/** Saved media-matching prefs win over Agent session defaults on reconnect. */
export function mergeAgentDiscoverySettings(
  saved: { agentModelId: string; agentReasoningEffort: string },
  result: AgentModelDiscoveryResult,
): { agentModelId?: string; agentReasoningEffort?: string } {
  if (!result.connected) return {};

  const patch: { agentModelId?: string; agentReasoningEffort?: string } = {};
  const models = result.options.models.map((option) => option.value);

  if (!saved.agentModelId.trim()) {
    const fallback =
      result.options.currentModelId?.trim() ||
      models[0] ||
      "";
    if (fallback) patch.agentModelId = fallback;
  } else if (models.length > 0 && !models.includes(saved.agentModelId)) {
    // Keep saved choice even if not in the latest list (manual refresh still works).
  }

  if (!saved.agentReasoningEffort.trim()) {
    const fallback = result.options.currentReasoningEffort?.trim();
    if (fallback) patch.agentReasoningEffort = fallback;
  }

  return patch;
}

export function buildAgentModelOptions(
  connection: AgentModelDiscoveryResult | null,
  savedModelId: string,
): AcpSessionOption[] {
  const fromConnection = connection?.options.models ?? [];
  if (
    savedModelId.trim() &&
    !fromConnection.some((option) => option.value === savedModelId)
  ) {
    return [
      { value: savedModelId, name: `${savedModelId}（已保存）` },
      ...fromConnection,
    ];
  }
  return fromConnection;
}

export function buildAgentReasoningOptions(
  connection: AgentModelDiscoveryResult | null,
  savedEffort: string,
): AcpSessionOption[] {
  const fromConnection = connection?.options.reasoningEfforts ?? [];
  if (
    savedEffort.trim() &&
    !fromConnection.some((option) => option.value === savedEffort)
  ) {
    return [
      { value: savedEffort, name: `${savedEffort}（已保存）` },
      ...fromConnection,
    ];
  }
  return fromConnection;
}

export function isDirectResolverReady(input: {
  modelId: string;
  modelBaseUrl: string;
  modelApiKeySaved: boolean;
  pendingApiKey: string;
}): boolean {
  return Boolean(
    input.modelId.trim() &&
      input.modelBaseUrl.trim() &&
      (input.modelApiKeySaved || input.pendingApiKey.trim()),
  );
}

export function isAgentResolverReady(agentModelId: string): boolean {
  return Boolean(agentModelId.trim());
}
