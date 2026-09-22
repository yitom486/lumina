import type { AcpSessionModelOptions, AcpSessionOption } from "./types";

/**
 * 全应用统一的模型配置模块。
 *
 * 对外只认这一套：模型选项整形、会话默认合并、选择归一。composer
 *（`useAgentModelControls`）、字幕 workshop（`useSubtitleWorkshopModels`）、
 * 媒体库 agent 守护（`MediaLibraryPanel`）共用同一实现；以后加新 agent、
 * 改选项规则、给各 profile 配不同模型，只改这里，调用方不用动。
 *
 * 形态分两层，绝不混用：
 * - `ModelSelection`：store 里存的样子，允许空白（空白 = 用 Agent 默认）；
 * - `ResolvedModelSelection`：发往后端的样子，空白已压成 null。
 */

export type ModelSelection = {
  modelId: string;
  reasoningEffort: string;
};

export type ResolvedModelSelection = {
  modelId: string | null;
  reasoningEffort: string | null;
};

function clean(value: string | null | undefined): string {
  return value?.trim() ?? "";
}

/** 入口归一：去空格、null→""，脏输入进不来。 */
export function normalizeModelSelection(input: {
  modelId?: string | null;
  reasoningEffort?: string | null;
}): ModelSelection {
  return {
    modelId: clean(input.modelId),
    reasoningEffort: clean(input.reasoningEffort),
  };
}

/** 出口归一：发往后端前空白压成 null。调用方不许再手写 `?.trim() || null`。 */
export function resolveModelSelection(input: {
  modelId?: string | null;
  reasoningEffort?: string | null;
}): ResolvedModelSelection {
  const normalized = normalizeModelSelection(input);
  return {
    modelId: normalized.modelId ? normalized.modelId : null,
    reasoningEffort: normalized.reasoningEffort
      ? normalized.reasoningEffort
      : null,
  };
}

export function isModelSelectionEmpty(selection: ModelSelection): boolean {
  return selection.modelId === "" && selection.reasoningEffort === "";
}

/**
 * 选项整形：会话下发的列表 + 已保存值。已保存的不在列表里就顶一条
 * “已保存”，手动填的值不断档；断开连接（options 为空）时只剩已保存那条。
 */
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

/**
 * 默认合并：只在“没存”时用会话当前值补（模型再退到列表首个）。
 * 已存的值永远不动。连通性由调用方自己先判（discovery 的 connected、
 * optionSource 非空），这里只认 options 有没有。
 */
export function mergeModelDefaults(
  saved: ModelSelection,
  options: AcpSessionModelOptions | null | undefined,
): Partial<ModelSelection> {
  if (!options) return {};

  const patch: Partial<ModelSelection> = {};
  const models = options.models.map((option) => option.value);

  if (!saved.modelId.trim()) {
    const fallback = options.currentModelId?.trim() || models[0] || "";
    if (fallback) patch.modelId = fallback;
  }

  if (!saved.reasoningEffort.trim()) {
    const fallback = options.currentReasoningEffort?.trim();
    if (fallback) patch.reasoningEffort = fallback;
  }

  return patch;
}

/**
 * 下拉显示名：命中列表用列表名，否则回退为空文案。
 * 空文案调用方定（composer 是“Agent 默认”，workshop 是“默认”），
 * 文案差异保留，逻辑统一。
 */
export function selectedOptionLabel(
  options: AcpSessionOption[],
  value: string,
  emptyLabel: string,
): string {
  return (
    options.find((option) => option.value === value)?.name ??
    (value.trim() ? value : emptyLabel)
  );
}
