import {
  normalizeAssistantBlocks,
  parseAssistantBlocksText,
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

const FALLBACK_VERSIONS: Record<AcpTaskId, string> = {
  chapter_recap: "chapter_recap.v1",
  chapter_outlook: "chapter_outlook.v1",
  plot_summary: "plot_summary.v1",
  question_candidates: "question_candidates.v1",
};

/**
 * Live backend contracts keyed by task id. Populated once from
 * `acp_task_contracts` (owned by the Rust prompt repository). The fallback
 * literals above exist only for offline/tests; at runtime the backend map
 * wins so the two sides can never drift into a second source of truth.
 */
let dynamicVersions: Partial<Record<AcpTaskId, string>> = {};

export function setTaskContractVersions(
  contracts: readonly { taskId: string; outputContractVersion: string }[],
): void {
  const next: Partial<Record<AcpTaskId, string>> = {};
  for (const contract of contracts) {
    if (isTaskId(contract.taskId) && contract.outputContractVersion) {
      next[contract.taskId] = contract.outputContractVersion;
    }
  }
  dynamicVersions = next;
}

export function resetTaskContractVersions(): void {
  dynamicVersions = {};
}

function expectedVersion(taskId: AcpTaskId): string {
  return dynamicVersions[taskId] ?? FALLBACK_VERSIONS[taskId];
}

function findTaskByVersion(version: string): AcpTaskId | undefined {
  const ids = Object.keys(FALLBACK_VERSIONS) as AcpTaskId[];
  const exact = ids.find((taskId) => expectedVersion(taskId) === version);
  if (exact) return exact;
  // 版本漂移（后端先发版或模型自带新戳）：同任务前缀即认身份，
  // 渲染走通用解析保底，不整单吞掉。跨任务串味仍拒绝。
  const familial = ids.find((taskId) => version.split(".")[0] === taskId);
  if (familial) {
    warnOnce(
      `[acp] shortcut contract drift: stamped ${version}, registry expects ${expectedVersion(familial)}`,
      `drift:${version}`,
    );
  }
  return familial;
}

function isTaskId(value: string): value is AcpTaskId {
  return Object.prototype.hasOwnProperty.call(FALLBACK_VERSIONS, value);
}

/**
 * Canonical Chinese labels for the four companion shortcuts.
 * Frontend-owned UI copy (not a backend contract): the single copy every
 * shortcut surface imports instead of maintaining its own literal map.
 */
export const SHORTCUT_TASK_LABELS: Record<AcpTaskId, string> = {
  chapter_recap: "本段总结",
  chapter_outlook: "后续看点",
  plot_summary: "剧情梳理",
  question_candidates: "观众问题",
};

const TASK_LABELS = SHORTCUT_TASK_LABELS;

const MAX_ITEMS = 8;
const MAX_SEEK_ACTIONS = 4;

/** Internal source keys must never leak to users; unknown keys stay as-is. */
const SOURCE_LABELS: Record<string, string> = {
  library_context: "剧集简介",
};

const TIMESTAMP_PATTERN = /\d{1,3}:\d{2}(?::\d{2})?/g;

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
  // The chat renderer also accepts its own closed-world `{ blocks: [...] }`
  // envelope. Let that envelope continue through parseAssistantBlocksText;
  // it is not one of the task contracts handled by this adapter.
  if (Array.isArray(value.blocks)) return null;
  const stamped = readContractVersion(value);
  if (stamped !== expectedVersion(taskId)) {
    // 同任务前缀的版本漂移（任一方向）：形态对就渲染，不整单吞掉；
    // 跨任务串味（别家任务的 JSON 落到本任务槽位）仍拒绝，避免张冠李戴。
    if (stamped === null || stamped.split(".")[0] !== taskId) {
      return fallback(taskId);
    }
    warnOnce(
      `[acp] shortcut contract drift for ${taskId}: stamped ${stamped}, expected ${expectedVersion(taskId)}`,
      `drift:${taskId}:${stamped}`,
    );
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
 *
 * 版本注册表只决定“任务身份”，不决定“能不能渲染”：先看通用结构化解析
 * 能不能产出安全块（allowlist 投影），能就直接展示。这样后端发版把契约
 * 从 v1 升到 v2 时，旧前端不会满屏“无法展示”，而是降级为通用结构化文档。
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
  if (Array.isArray(value.blocks)) return { answer };
  const parsed = parseAssistantBlocksText(answer);
  const version = readContractVersion(value);
  const shortcutTaskId = version ? findTaskByVersion(version) : undefined;
  if (parsed?.blocks.length) return { answer, shortcutTaskId };
  if (shortcutTaskId) {
    return {
      answer: `${TASK_LABELS[shortcutTaskId]}结果暂时无法展示，请稍后重试。`,
    };
  }
  if (version !== null) logUnknownContractVersion(version);
  return { answer: "该结构化结果暂时无法展示，请稍后重试。" };
}

function logUnknownContractVersion(version: string): void {
  // 只记协议 token，不记用户内容：下次再出现“无法展示”，devtools 里
  // 直接能看到是哪个版本号对不上。按版本去重，避免每次重渲染刷屏。
  warnOnce(
    `[acp] unrecognized shortcut contract version: ${version}`,
    `unknown:${version}`,
  );
}

const warnedKeys = new Set<string>();

function warnOnce(message: string, key: string): void {
  if (warnedKeys.has(key)) return;
  warnedKeys.add(key);
  if (typeof console !== "undefined") {
    console.warn(message);
  }
}

function contractCandidates(
  taskId: AcpTaskId,
  value: Record<string, unknown>,
): ReadonlyArray<Record<string, unknown>> {
  const evidence = normalizeEvidence(value.evidence);
  const scope =
    normalizeScope(value.scope) ??
    normalizeChapterScope(value.chapter) ??
    normalizeSpoilerBoundary(value.spoiler_boundary);
  const uncertainty = normalizeTextArray(value.uncertainty).map(
    (item) => `待确认：${item}`,
  );

  switch (taskId) {
    case "chapter_recap":
      return cardCandidates(taskId, value, {
        title: "本段总结",
        summaryKeys: ["summary", "recap", "content", "text"],
        bullets: [...evidence, ...uncertainty],
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
  const bullets = options.bullets.map(humanizeBulletTail).slice(0, MAX_ITEMS);
  return {
    kind: "watch-feed-card",
    id: `${taskId}-result`,
    eyebrow: options.scope,
    title: options.title,
    ...(summary ? { summary } : {}),
    bullets,
    spoilerLevel: "current",
    actions: collectSeekActions(taskId, bullets),
  };
}

function humanizeReference(reference: string): string {
  const key = reference.trim();
  return SOURCE_LABELS[key] ?? reference;
}

function humanizeBulletTail(bullet: string): string {
  const trimmed = bullet.trim();
  if (SOURCE_LABELS[trimmed]) return SOURCE_LABELS[trimmed];
  const separator = bullet.lastIndexOf("·");
  if (separator < 0) return bullet;
  const head = bullet.slice(0, separator).trimEnd();
  const tail = bullet.slice(separator + 1).trim();
  const mapped = SOURCE_LABELS[tail];
  if (!mapped || !head) return bullet;
  return `${head} · ${mapped}`;
}

function collectSeekActions(
  taskId: string,
  bullets: readonly string[],
): Array<Record<string, unknown>> {
  const seen = new Set<number>();
  const actions: Array<Record<string, unknown>> = [];
  for (const bullet of bullets) {
    const matches = bullet.match(TIMESTAMP_PATTERN);
    if (!matches) continue;
    for (const timestamp of matches) {
      const startMs = timestampToMs(timestamp);
      if (startMs === null || seen.has(startMs)) continue;
      seen.add(startMs);
      actions.push({
        id: `${taskId}-seek-${actions.length}`,
        label: `跳转到 ${timestamp}`,
        action: { type: "seek", anchor: { startMs } },
      });
      if (actions.length >= MAX_SEEK_ACTIONS) return actions;
    }
  }
  return actions;
}

function timestampToMs(timestamp: string): number | null {
  const parts = timestamp.split(":").map((part) => Number(part));
  if (parts.some((part) => !Number.isFinite(part) || part < 0)) return null;
  if (parts.length === 2) {
    const [minutes, seconds] = parts as [number, number];
    if (seconds > 59) return null;
    return (minutes * 60 + seconds) * 1000;
  }
  if (parts.length === 3) {
    const [hours, minutes, seconds] = parts as [number, number, number];
    if (minutes > 59 || seconds > 59) return null;
    return (hours * 3600 + minutes * 60 + seconds) * 1000;
  }
  return null;
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
        // 一点即问：无锚点，执行时取当前播放位置。
        options: [
          {
            id: `question-${index}-ask`,
            label: "直接问",
            action: { type: "ask", prompt: question },
          },
        ],
      },
    ];
  });
}

function normalizeEvidence(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.slice(0, MAX_ITEMS).flatMap((item) => {
    if (typeof item === "string") {
      const text = readText(item);
      return text ? [humanizeBulletTail(text)] : [];
    }
    if (!isRecord(item)) return [];
    const reference = firstText(item, ["ref", "reference", "timestamp", "time_range"]);
    const text = firstText(item, [
      "text",
      "quote",
      "label",
      "title",
      "description",
      "summary",
      "fact",
    ]);
    if (text)
      return [reference ? `${text} · ${humanizeReference(reference)}` : text];
    if (reference) return [humanizeReference(reference)];
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

function normalizeChapterScope(value: unknown): string | undefined {
  if (!isRecord(value)) return undefined;
  const title = firstText(value, ["title", "name"]);
  const position = firstText(value, ["position", "timestamp"]);
  if (title && position) return `${title} · ${position}`;
  return title ?? position ?? undefined;
}

function normalizeSpoilerBoundary(value: unknown): string | undefined {
  switch (readString(value)) {
    case "current_position":
      return "截至当前播放位置";
    case "current_chapter":
      return "截至当前章节";
    case "none":
      return "不剧透";
    default:
      return undefined;
  }
}

function readContractVersion(value: Record<string, unknown>): string | null {
  return readString(value.version) ?? readString(value.contract);
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
