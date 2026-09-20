import {
  normalizeAssistantBlocks,
  type AssistantBlock,
} from "@lumina/chat-ui/assistantBlocks";

import type { AcpTaskId } from "./api";

type ShortcutOutput = {
  blocks: AssistantBlock[];
  fallbackText?: string;
};

export type RestoredShortcutOutput = {
  answer: string;
  shortcutTaskId?: AcpTaskId;
};

const EXPECTED_VERSIONS: Record<AcpTaskId, string> = {
  chapter_recap: "chapter_recap.v1",
  chapter_outlook: "chapter_outlook.v1",
  plot_summary: "plot_summary.v1",
  question_candidates: "question_candidates.v1",
};

const TASK_LABELS: Record<AcpTaskId, string> = {
  chapter_recap: "本段总结",
  chapter_outlook: "后续看点",
  plot_summary: "剧情梳理",
  question_candidates: "观众问题",
};

const MAX_ITEMS = 8;

/**
 * Adapt one known companion contract into the closed chat block union.
 *
 * This boundary deliberately accepts unknown data only as `unknown`, then
 * reads a small allowlist of fields. It never forwards the contract object or
 * arbitrary keys to JSX. A null result means the answer was ordinary text and
 * should use the normal Markdown path.
 */
export function adaptShortcutOutput(
  taskId: AcpTaskId,
  answer: string,
  options?: { streaming?: boolean },
): ShortcutOutput | null {
  const source = answer.trim();
  if (!looksLikeJson(source)) return null;

  let value: unknown;
  try {
    value = JSON.parse(stripJsonFence(source)) as unknown;
  } catch {
    return options?.streaming ? null : fallback(taskId);
  }

  if (!isRecord(value)) return fallback(taskId);
  if (readString(value.version) !== EXPECTED_VERSIONS[taskId]) {
    return fallback(taskId);
  }

  const candidates = contractCandidates(taskId, value);
  const blocks = normalizeAssistantBlocks(candidates);
  if (blocks.length > 0) return { blocks };
  return options?.streaming ? null : fallback(taskId);
}

/**
 * Rebuild a rich-output identity from a completed persisted answer.
 * Unknown JSON deliberately becomes a business fallback instead of Markdown:
 * restoration must never expose an arbitrary object in a chat bubble.
 */
export function normalizeRestoredShortcutOutput(
  answer: string,
): RestoredShortcutOutput {
  const source = answer.trim();
  if (!looksLikeJson(source)) return { answer };

  let value: unknown;
  try {
    value = JSON.parse(stripJsonFence(source)) as unknown;
  } catch {
    return { answer: "该结构化结果暂时无法展示，请稍后重试。" };
  }
  if (!isRecord(value)) {
    return { answer: "该结构化结果暂时无法展示，请稍后重试。" };
  }
  const version = readString(value.version);
  const shortcutTaskId = (Object.keys(EXPECTED_VERSIONS) as AcpTaskId[]).find(
    (taskId) => EXPECTED_VERSIONS[taskId] === version,
  );
  if (!shortcutTaskId) {
    return { answer: "该结构化结果暂时无法展示，请稍后重试。" };
  }
  return { answer, shortcutTaskId };
}

function contractCandidates(
  taskId: AcpTaskId,
  value: Record<string, unknown>,
): ReadonlyArray<Record<string, unknown>> {
  const evidence = normalizeEvidence(value.evidence);
  const scope = normalizeScope(value.scope);

  switch (taskId) {
    case "chapter_recap":
      return cardCandidates(taskId, value, {
        title: "本段总结",
        summaryKeys: ["summary", "recap", "content", "text"],
        bullets: evidence,
        scope,
      });
    case "chapter_outlook":
      return cardCandidates(taskId, value, {
        title: "后续看点",
        summaryKeys: ["summary", "outlook", "content", "text"],
        bullets: [
          ...normalizeTextArray(value.items),
          ...normalizeTextArray(value.points),
          ...normalizeTextArray(value.outlook),
          ...evidence,
        ].slice(0, MAX_ITEMS),
        scope,
      });
    case "plot_summary":
      return cardCandidates(taskId, value, {
        title: "剧情梳理",
        summaryKeys: ["summary", "content", "text"],
        bullets: evidence,
        scope,
      });
    case "question_candidates":
      return normalizeQuestions(value);
  }
}

function cardCandidates(
  taskId: AcpTaskId,
  value: Record<string, unknown>,
  options: {
    title: string;
    summaryKeys: readonly string[];
    bullets: string[];
    scope: string | undefined;
  },
): ReadonlyArray<Record<string, unknown>> {
  const summary = firstText(value, options.summaryKeys);
  if (!summary && options.bullets.length === 0) return [];
  return [watchCard(taskId, value, options)];
}

function watchCard(
  taskId: AcpTaskId,
  value: Record<string, unknown>,
  options: {
    title: string;
    summaryKeys: readonly string[];
    bullets: string[];
    scope: string | undefined;
  },
): Record<string, unknown> {
  const summary = firstText(value, options.summaryKeys);
  return {
    kind: "watch-feed-card",
    id: `${taskId}-result`,
    eyebrow: options.scope,
    title: options.title,
    ...(summary ? { summary } : {}),
    bullets: options.bullets.slice(0, MAX_ITEMS),
    spoilerLevel: "current",
    actions: [],
  };
}

function normalizeQuestions(
  value: Record<string, unknown>,
): ReadonlyArray<Record<string, unknown>> {
  const rawItems = [value.questions, value.candidates].find(
    Array.isArray,
  ) as unknown[] | undefined;
  if (!rawItems) return [];

  return rawItems.slice(0, MAX_ITEMS).flatMap((item, index) => {
    const question =
      typeof item === "string"
        ? readText(item)
        : isRecord(item)
          ? firstText(item, ["question", "prompt", "text", "title"])
          : null;
    if (!question) return [];
    return [
      {
        kind: "question-card",
        id: `question-${index}`,
        question,
        options: [],
      },
    ];
  });
}

function normalizeEvidence(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.slice(0, MAX_ITEMS).flatMap((item) => {
    if (typeof item === "string") {
      const text = readText(item);
      return text ? [text] : [];
    }
    if (!isRecord(item)) return [];
    const text = firstText(item, [
      "text",
      "quote",
      "label",
      "title",
      "description",
      "summary",
    ]);
    if (text) return [text];
    const kind = readString(item.kind);
    const kindLabel =
      kind === "transcript"
        ? "字幕证据"
        : kind === "screenshot"
          ? "画面证据"
          : kind === "chapter"
            ? "章节证据"
            : null;
    return kindLabel ? [kindLabel] : [];
  });
}

function normalizeScope(value: unknown): string | undefined {
  if (!isRecord(value)) return undefined;
  return (
    firstText(value, ["label", "title", "chapter_title", "name"]) ??
    undefined
  );
}

function normalizeTextArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.slice(0, MAX_ITEMS).flatMap((item) => {
    if (typeof item === "string") {
      const text = readText(item);
      return text ? [text] : [];
    }
    if (!isRecord(item)) return [];
    const text = firstText(item, ["text", "title", "summary", "description"]);
    return text ? [text] : [];
  });
}

function fallback(taskId: AcpTaskId): ShortcutOutput {
  return {
    blocks: [],
    fallbackText: `${TASK_LABELS[taskId]}结果暂时无法展示，请稍后重试。`,
  };
}

function firstText(
  value: Record<string, unknown>,
  keys: readonly string[],
): string | null {
  for (const key of keys) {
    const text = readText(value[key]);
    if (text) return text;
  }
  return null;
}

function readText(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const text = value.split("\u0000").join("").trim();
  return text ? text.slice(0, 20_000) : null;
}

function readString(value: unknown): string | null {
  return typeof value === "string" ? value.trim() || null : null;
}

function looksLikeJson(value: string): boolean {
  return (
    (value.startsWith("{") && value.endsWith("}")) ||
    (value.startsWith("[") && value.endsWith("]")) ||
    /^```json\s/i.test(value)
  );
}

function stripJsonFence(value: string): string {
  const match = /^```json\s*\r?\n([\s\S]*?)\r?\n```$/i.exec(value);
  return match?.[1]?.trim() ?? value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
