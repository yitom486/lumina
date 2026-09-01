import type { AcpSessionModelOptions, AcpSessionOption } from "./types";

export function buildModelOptions(
  options: AcpSessionModelOptions | null | undefined,
  savedModelId: string,
): AcpSessionOption[] {
  const fromSession = options?.models ?? [];
  if (
    savedModelId.trim() &&
    !fromSession.some((option) => option.value === savedModelId)
  ) {
    return [
      { value: savedModelId, name: `${savedModelId}（已保存）` },
      ...fromSession,
    ];
  }
  return fromSession;
}

export function buildReasoningOptions(
  options: AcpSessionModelOptions | null | undefined,
  savedEffort: string,
): AcpSessionOption[] {
  const fromSession = options?.reasoningEfforts ?? [];
  if (
    savedEffort.trim() &&
    !fromSession.some((option) => option.value === savedEffort)
  ) {
    return [
      { value: savedEffort, name: `${savedEffort}（已保存）` },
      ...fromSession,
    ];
  }
  return fromSession;
}

export function mergeSessionModelDefaults(
  saved: { modelId: string; reasoningEffort: string },
  options: AcpSessionModelOptions | null | undefined,
): { modelId?: string; reasoningEffort?: string } {
  if (!options) return {};

  const patch: { modelId?: string; reasoningEffort?: string } = {};
  const models = options.models.map((option) => option.value);

  if (!saved.modelId.trim()) {
    const fallback =
      options.currentModelId?.trim() || models[0] || "";
    if (fallback) patch.modelId = fallback;
  }

  if (!saved.reasoningEffort.trim()) {
    const fallback = options.currentReasoningEffort?.trim();
    if (fallback) patch.reasoningEffort = fallback;
  }

  return patch;
}
