import type {
  AcpSessionModelOptions,
  AcpSessionOption,
  SessionConfigOption,
} from "./types";

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

/**
 * 参数化 Agent（cursor）的配置维度归类。对照 Cursor 原生五维度：
 * Agent 模式 / Model / Effort / Context / Fast 开关。
 *
 * 匹配规则只认 agent 下发的东西（category/id/name），缺席的维度就是
 * 该模型没下发——渲染为缺席，绝不硬编假选项：
 * - mode：select 且 category==='mode'，或 id/name 含 mode（collab 除外）；
 * - model：select 且 category==='model'，或 id 含独立 model 词；
 * - effort：select 且 id 含 thought/reason/effort，或 category==='thought_level'；
 * - context：select 且 category==='context'，或 id/name 含 context/ctx；
 * - fastToggle：boolean 且 id 含 fast（category 不限）→ 输入栏可见开关；
 * - fastSelect：select 且 id 含 fast → 进「更多设置」。
 * 展示顺序固定 mode0/model1/effort2/context3；同维度只取首个命中，
 * 其余与未归类的一起进 others（暂不渲染，不断言）。
 */
export type CursorConfigDimensions = {
  mode?: SessionConfigOption;
  model?: SessionConfigOption;
  effort?: SessionConfigOption;
  context?: SessionConfigOption;
  fastToggle?: SessionConfigOption;
  fastSelect?: SessionConfigOption;
  others: SessionConfigOption[];
};

function isSelect(option: SessionConfigOption): boolean {
  return option.kind.kind === "select";
}

function textOf(value: string | null | undefined): string {
  return value?.trim().toLowerCase() ?? "";
}

function containsWord(haystack: string, word: string): boolean {
  return haystack
    .split(/[^a-z0-9]+/)
    .some((token) => token === word);
}

export function classifyCursorDimensions(
  options: readonly SessionConfigOption[] | null | undefined,
): CursorConfigDimensions {
  const result: CursorConfigDimensions = { others: [] };
  if (!options) return result;
  const taken = new Set<SessionConfigOption>();

  const takeFirst = (
    predicate: (option: SessionConfigOption) => boolean,
  ): SessionConfigOption | undefined => {
    const found = options.find(
      (option) => !taken.has(option) && isSelect(option) && predicate(option),
    );
    if (found) taken.add(found);
    return found;
  };

  const mode = takeFirst((option) => {
    if (textOf(option.category) === "mode") return true;
    // 独立 mode 词（"model" 含 mode 子串，必须按词切分，否则 mode 会吞掉 model）。
    const text = `${textOf(option.id)} ${textOf(option.name)}`;
    return containsWord(text, "mode") && !text.includes("collab");
  });
  if (mode) result.mode = mode;

  const model = takeFirst((option) => {
    if (textOf(option.category) === "model") return true;
    return containsWord(textOf(option.id), "model");
  });
  if (model) result.model = model;

  const effort = takeFirst((option) => {
    if (textOf(option.category) === "thought_level") return true;
    // "thinking" 不含 "thought" 子串，think/thought 都要认。
    const id = textOf(option.id);
    return (
      id.includes("think") ||
      id.includes("thought") ||
      id.includes("reason") ||
      id.includes("effort")
    );
  });
  if (effort) result.effort = effort;

  const context = takeFirst((option) => {
    if (textOf(option.category) === "context") return true;
    const text = `${textOf(option.id)} ${textOf(option.name)}`;
    return text.includes("context") || /(^|[^a-z])ctx([^a-z]|$)/.test(text);
  });
  if (context) result.context = context;

  const fastToggle = options.find(
    (option) =>
      !taken.has(option) &&
      option.kind.kind === "boolean" &&
      textOf(option.id).includes("fast"),
  );
  if (fastToggle) {
    taken.add(fastToggle);
    result.fastToggle = fastToggle;
  }

  const fastSelect = options.find(
    (option) =>
      !taken.has(option) && isSelect(option) && textOf(option.id).includes("fast"),
  );
  if (fastSelect) {
    taken.add(fastSelect);
    result.fastSelect = fastSelect;
  }

  result.others = options.filter((option) => !taken.has(option));
  return result;
}

/**
 * listed 门槛：只下发 agent advertised 的原值。select 的 value 必须在
 * options 里，boolean 的 "true"/"false" 永远可发。未命中一律不发——
 * 拼未 listed 值会被拒收（`Invalid params`），这是之前连接即报错的根因。
 */
export function isListedConfigValue(
  option: SessionConfigOption,
  value: string,
): boolean {
  if (option.kind.kind === "boolean") {
    const normalized = value.trim().toLowerCase();
    return normalized === "true" || normalized === "false";
  }
  if (option.kind.kind === "select") {
    return option.kind.options.some((item) => item.value === value);
  }
  return false;
}

/** select 维度当前值（缺席即 ""，表示不动、沿用 agent 默认）。 */
export function currentSelectValue(option: SessionConfigOption): string {
  if (option.kind.kind !== "select") return "";
  return option.kind.current?.trim() ?? "";
}

/** boolean 维度当前值（缺席即 false）。 */
export function currentBooleanValue(option: SessionConfigOption): boolean {
  return option.kind.kind === "boolean" && option.kind.current;
}
