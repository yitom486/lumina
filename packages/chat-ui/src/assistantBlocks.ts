export type AssistantAnchor =
  | {
      startMs: number;
      endMs?: number;
      chapterId?: string;
    }
  | {
      chapterId: string;
      startMs?: undefined;
      endMs?: undefined;
    };

export type AssistantAction =
  | { type: "seek"; anchor: Extract<AssistantAnchor, { startMs: number }> }
  | {
      type: "save-note";
      anchor: AssistantAnchor;
      content: string;
    }
  | {
      type: "ask";
      anchor: AssistantAnchor;
      prompt: string;
    };

export type SpoilerLevel = "none" | "current" | "future";
export type AgentTaskStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

export type AssistantActionChip = {
  id: string;
  label: string;
  action: AssistantAction;
  disabled?: boolean;
};

export type NarrativeBlock = {
  kind: "narrative";
  id: string;
  format: "markdown";
  markdown: string;
};

export type TranscriptQuoteBlock = {
  kind: "transcript-quote";
  id: string;
  quote: string;
  speaker?: string;
  anchor: AssistantAnchor;
};

export type TimelineItem = {
  id: string;
  atMs: number;
  title: string;
  summary?: string;
  anchor?: AssistantAnchor;
};

export type TimelineBlock = {
  kind: "timeline";
  id: string;
  title: string;
  items: TimelineItem[];
};

export type WatchFeedCardBlock = {
  kind: "watch-feed-card";
  id: string;
  eyebrow?: string;
  title: string;
  summary?: string;
  bullets: string[];
  spoilerLevel: SpoilerLevel;
  anchor?: AssistantAnchor;
  actions: AssistantActionChip[];
};

export type QuestionOption = AssistantActionChip;

export type QuestionCardBlock = {
  kind: "question-card";
  id: string;
  question: string;
  options: QuestionOption[];
  anchor?: AssistantAnchor;
};

export type AgentTaskStatusBlock = {
  kind: "agent-task-status";
  id: string;
  taskId: string;
  title: string;
  status: AgentTaskStatus;
  message?: string;
  progress?: number;
};

export type ActionChipBlock = {
  kind: "action-chip";
  id: string;
  label: string;
  action: AssistantAction;
  disabled?: boolean;
};

/** The only block shapes that RichBlockRenderer is allowed to receive. */
export type AssistantBlock =
  | NarrativeBlock
  | TranscriptQuoteBlock
  | TimelineBlock
  | WatchFeedCardBlock
  | QuestionCardBlock
  | AgentTaskStatusBlock
  | ActionChipBlock;

export type AssistantBlockInput =
  | string
  | Readonly<Record<string, unknown>>
  | readonly unknown[];

export type AssistantBlockNormalizationNotice = {
  index: number;
  reason:
    | "incomplete"
    | "invalid"
    | "unknown-kind"
    | "truncated";
  fallback: boolean;
};

export type AssistantBlockNormalizationResult = {
  blocks: AssistantBlock[];
  notices: AssistantBlockNormalizationNotice[];
};

const MAX_BLOCKS = 100;
const MAX_ACTIONS = 4;
const MAX_TIMELINE_ITEMS = 40;
const MAX_QUESTION_OPTIONS = 6;
const MAX_TEXT_LENGTH = 20_000;
const MAX_SHORT_TEXT_LENGTH = 500;
const MAX_ID_LENGTH = 120;

/**
 * Normalize an untrusted assistant payload into the closed block union.
 * Unknown, malformed, or incomplete objects never reach JSX as dynamic data.
 */
export function normalizeAssistantBlocks(
  input: unknown,
  options?: { streaming?: boolean },
): AssistantBlock[] {
  return normalizeAssistantBlocksWithNotices(input, options).blocks;
}

export function normalizeAssistantBlocksWithNotices(
  input: unknown,
  options?: { streaming?: boolean },
): AssistantBlockNormalizationResult {
  const notices: AssistantBlockNormalizationNotice[] = [];
  const candidates = extractCandidates(input);
  const blocks: AssistantBlock[] = [];

  candidates.slice(0, MAX_BLOCKS).forEach((candidate, index) => {
    const block = normalizeCandidate(candidate, index, options, notices);
    if (block) blocks.push(block);
  });

  if (candidates.length > MAX_BLOCKS) {
    notices.push({
      index: MAX_BLOCKS,
      reason: "truncated",
      fallback: false,
    });
  }

  return { blocks, notices };
}

/**
 * Parse only a complete structured assistant payload.
 *
 * Ordinary Markdown is intentionally not inspected for JSON-looking fragments:
 * callers should keep rendering it through the normal Markdown path. During a
 * stream, an incomplete structured payload also falls back to that path.
 */
export function parseAssistantBlocksText(
  text: string,
  options?: { streaming?: boolean },
): AssistantBlockNormalizationResult | null {
  const value = parseCompleteStructuredJson(text);
  if (value === null) return null;

  const result = normalizeAssistantBlocksWithNotices(value, options);
  if (
    options?.streaming &&
    result.notices.some((notice) => notice.reason === "incomplete")
  ) {
    return null;
  }
  return result.blocks.length > 0 ? result : null;
}

function parseCompleteStructuredJson(text: string): unknown | null {
  const trimmed = text.trim();
  if (!trimmed) return null;

  const fenced = /^```json\s*\r?\n([\s\S]*?)\r?\n```$/i.exec(trimmed);
  const source = fenced?.[1]?.trim() ?? trimmed;
  const looksLikeJson =
    (source.startsWith("{") && source.endsWith("}")) ||
    (source.startsWith("[") && source.endsWith("]"));
  if (!looksLikeJson) return null;

  try {
    const value: unknown = JSON.parse(source);
    return isRecord(value) || Array.isArray(value) ? value : null;
  } catch {
    return null;
  }
}

function extractCandidates(input: unknown): unknown[] {
  if (Array.isArray(input)) return input;
  if (isRecord(input) && Array.isArray(input.blocks)) return input.blocks;
  return [input];
}

function normalizeCandidate(
  input: unknown,
  index: number,
  options: { streaming?: boolean } | undefined,
  notices: AssistantBlockNormalizationNotice[],
): AssistantBlock | null {
  if (typeof input === "string") {
    return narrativeBlock(`narrative-${index}`, input);
  }

  if (!isRecord(input)) {
    notices.push({ index, reason: "invalid", fallback: false });
    return null;
  }

  if (isIncomplete(input, options)) {
    const fallback = fallbackNarrative(input, index);
    notices.push({ index, reason: "incomplete", fallback: Boolean(fallback) });
    return fallback;
  }

  const rawKind = readString(input.kind) ?? readString(input.type);
  const kind = normalizeKind(rawKind);
  if (!kind) {
    const fallback = fallbackNarrative(input, index);
    notices.push({ index, reason: "unknown-kind", fallback: Boolean(fallback) });
    return fallback;
  }

  const id = readId(input.id, `${kind}-${index}`);
  const block = normalizeKnownBlock(kind, input, id);
  if (!block) {
    const fallback = fallbackNarrative(input, index);
    notices.push({ index, reason: "invalid", fallback: Boolean(fallback) });
    return fallback;
  }
  return block;
}

function normalizeKnownBlock(
  kind: AssistantBlock["kind"],
  input: Record<string, unknown>,
  id: string,
): AssistantBlock | null {
  switch (kind) {
    case "narrative": {
      const markdown = readText(input.markdown) ?? readText(input.text) ?? readText(input.content);
      return markdown ? narrativeBlock(id, markdown) : null;
    }
    case "transcript-quote": {
      const quote = readText(input.quote) ?? readText(input.text);
      const anchor = normalizeAnchor(input.anchor, input);
      if (!quote || !anchor) return null;
      return {
        kind,
        id,
        quote,
        speaker: readShortText(input.speaker) ?? undefined,
        anchor,
      };
    }
    case "timeline": {
      const rawItems = Array.isArray(input.items) ? input.items : [];
      const items = rawItems
        .slice(0, MAX_TIMELINE_ITEMS)
        .map((item, itemIndex) => normalizeTimelineItem(item, `${id}-${itemIndex}`))
        .filter((item): item is TimelineItem => Boolean(item));
      if (items.length === 0) return null;
      return {
        kind,
        id,
        title: readShortText(input.title) ?? "时间线",
        items,
      };
    }
    case "watch-feed-card": {
      const title = readShortText(input.title) ?? readShortText(input.headline);
      if (!title) return null;
      return {
        kind,
        id,
        eyebrow: readShortText(input.eyebrow) ?? undefined,
        title,
        summary: readText(input.summary) ?? readText(input.content) ?? undefined,
        bullets: normalizeTextArray(input.bullets, 5),
        spoilerLevel: normalizeSpoilerLevel(input.spoilerLevel ?? input.spoiler),
        anchor: normalizeAnchor(input.anchor, input) ?? undefined,
        actions: normalizeActionChips(input.actions, id),
      };
    }
    case "question-card": {
      const question = readText(input.question) ?? readText(input.prompt);
      if (!question) return null;
      const options = normalizeActionChips(input.options, id, MAX_QUESTION_OPTIONS);
      return {
        kind,
        id,
        question,
        options,
        anchor: normalizeAnchor(input.anchor, input) ?? undefined,
      };
    }
    case "agent-task-status": {
      const taskId = readShortText(input.taskId) ?? readShortText(input.task_id);
      const status = normalizeTaskStatus(input.status);
      const title = readShortText(input.title) ?? readShortText(input.label);
      if (!taskId || !status || !title) return null;
      const progress = normalizeProgress(input.progress);
      return {
        kind,
        id,
        taskId,
        title,
        status,
        message: readText(input.message) ?? undefined,
        ...(progress === undefined ? {} : { progress }),
      };
    }
    case "action-chip": {
      const label = readShortText(input.label) ?? readShortText(input.title);
      const action = normalizeAction(input.action ?? input);
      if (!label || !action) return null;
      return {
        kind,
        id,
        label,
        action,
        disabled: input.disabled === true,
      };
    }
  }
}

function normalizeTimelineItem(input: unknown, id: string): TimelineItem | null {
  if (!isRecord(input)) return null;
  const title = readShortText(input.title) ?? readShortText(input.label);
  const atMs = normalizeTime(input.atMs ?? input.startMs ?? input.positionMs);
  if (!title || atMs === null) return null;
  return {
    id: readId(input.id, id),
    atMs,
    title,
    summary: readText(input.summary) ?? readText(input.description) ?? undefined,
    anchor: normalizeAnchor(input.anchor, input) ?? undefined,
  };
}

function normalizeActionChips(
  input: unknown,
  idPrefix: string,
  maxItems = MAX_ACTIONS,
): AssistantActionChip[] {
  if (!Array.isArray(input)) return [];
  return input
    .slice(0, maxItems)
    .map((item, index): AssistantActionChip | null => {
      if (!isRecord(item)) return null;
      const label = readShortText(item.label) ?? readShortText(item.title);
      const action = normalizeAction(item.action ?? item);
      if (!label || !action) return null;
      return {
        id: readId(item.id, `${idPrefix}-action-${index}`),
        label,
        action,
        ...(item.disabled === true ? { disabled: true } : {}),
      };
    })
    .filter((item): item is AssistantActionChip => Boolean(item));
}

function normalizeAction(input: unknown): AssistantAction | null {
  if (!isRecord(input)) return null;
  const type = readString(input.type) ?? readString(input.kind);
  const anchor = normalizeAnchor(input.anchor, input);
  if (!type || !anchor) return null;

  switch (type) {
    case "seek":
      return hasStartMs(anchor) ? { type, anchor } : null;
    case "save-note": {
      const content = readText(input.content) ?? readText(input.text);
      return content ? { type, anchor, content } : null;
    }
    case "ask": {
      const prompt = readText(input.prompt) ?? readText(input.text);
      return prompt ? { type, anchor, prompt } : null;
    }
    default:
      return null;
  }
}

function normalizeAnchor(
  value: unknown,
  fallback: Record<string, unknown>,
): AssistantAnchor | null {
  const source = isRecord(value) ? value : fallback;
  const chapterId = readShortText(source.chapterId) ?? readShortText(source.chapter_id);
  const startMs = normalizeTime(source.startMs ?? source.atMs ?? source.positionMs);
  const endMs = normalizeTime(source.endMs);

  if (startMs !== null) {
    if (endMs !== null && endMs < startMs) return null;
    return {
      startMs,
      ...(endMs === null ? {} : { endMs }),
      ...(chapterId ? { chapterId } : {}),
    };
  }
  return chapterId ? { chapterId } : null;
}

function normalizeKind(value: string | null): AssistantBlock["kind"] | null {
  switch (value) {
    case "narrative":
    case "markdown":
      return "narrative";
    case "transcript-quote":
    case "transcript_quote":
      return "transcript-quote";
    case "timeline":
      return "timeline";
    case "watch-feed-card":
    case "watch_feed_card":
      return "watch-feed-card";
    case "question-card":
    case "question_card":
      return "question-card";
    case "agent-task-status":
    case "agent_task_status":
      return "agent-task-status";
    case "action-chip":
    case "action_chip":
      return "action-chip";
    default:
      return null;
  }
}

function narrativeBlock(id: string, markdown: string): NarrativeBlock | null {
  const safeMarkdown = readText(markdown);
  return safeMarkdown
    ? { kind: "narrative", id, format: "markdown", markdown: safeMarkdown }
    : null;
}

function fallbackNarrative(
  input: Record<string, unknown>,
  index: number,
): NarrativeBlock | null {
  const text = readText(input.fallbackText) ?? readText(input.text) ?? readText(input.content);
  return text ? narrativeBlock(`fallback-${index}`, text) : null;
}

function isIncomplete(
  input: Record<string, unknown>,
  options: { streaming?: boolean } | undefined,
): boolean {
  if (input.complete === false || input.incomplete === true || input.streaming === true) {
    return true;
  }
  if (!options?.streaming) return false;
  const status = readString(input.status);
  return status === "streaming" || status === "partial" || status === "incomplete";
}

function normalizeSpoilerLevel(value: unknown): SpoilerLevel {
  switch (value) {
    case "none":
    case "current":
      return value;
    case "future":
    case "spoiler":
      return "future";
    default:
      return "none";
  }
}

function normalizeTaskStatus(value: unknown): AgentTaskStatus | null {
  switch (value) {
    case "queued":
    case "running":
    case "succeeded":
    case "failed":
    case "cancelled":
      return value;
    default:
      return null;
  }
}

function normalizeProgress(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isFinite(value)) return undefined;
  return Math.min(100, Math.max(0, value));
}

function normalizeTextArray(value: unknown, maxItems: number): string[] {
  if (!Array.isArray(value)) return [];
  return value
    .slice(0, maxItems)
    .map((item) => readShortText(item))
    .filter((item): item is string => Boolean(item));
}

function normalizeTime(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0
    ? value
    : null;
}

function readId(value: unknown, fallback: string): string {
  return readShortText(value, MAX_ID_LENGTH) ?? fallback;
}

function readText(value: unknown): string | null {
  return readString(value, MAX_TEXT_LENGTH);
}

function readShortText(value: unknown, maxLength = MAX_SHORT_TEXT_LENGTH): string | null {
  return readString(value, maxLength);
}

function readString(value: unknown, maxLength = MAX_TEXT_LENGTH): string | null {
  if (typeof value !== "string") return null;
  const text = value.split("\u0000").join("").trim();
  return text ? text.slice(0, maxLength) : null;
}

function hasStartMs(
  anchor: AssistantAnchor,
): anchor is Extract<AssistantAnchor, { startMs: number }> {
  return typeof anchor.startMs === "number";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
