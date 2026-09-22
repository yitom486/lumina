import type { ChatTurn } from "./types";

/**
 * 聊天秒开恢复（第 1 层：本地渲染缓存）。
 *
 * 对标 Inkdown 的 `zustand/persist` 层：面板挂载瞬间先渲染上次退出的
 * turns + 草稿，零网络；原生会话续接是第 2 层（persisted savedSession
 * + connect 时 resume），见 `acpSessionStore` 与 `AcpPanel` 的对账逻辑。
 *
 * 只存“可重渲染”的内容，不存瞬态：
 * - 流式中的 turn：有 answer 就冻结为 done，没有就整条丢掉；
 * - 粘贴图片 dataUrl、agentDraft/segments、未确认的批注提议、工具原文
 *   全部裁掉（体重 + 可能含本地路径）；
 * - 空 turn 不存，最多保留 40 条；
 * - 写 localStorage 节流 1500ms trailing，超配额就对半砍重试。
 */

export const CHAT_RESTORE_KEY = "lumina-acp-chat-restore";

const SNAPSHOT_VERSION = 1;
const MAX_TURNS = 40;
const MAX_TEXT_LENGTH = 20_000;
const WRITE_DELAY_MS = 1500;
const MAX_WRITE_ATTEMPTS = 3;

export type ChatRestoreSnapshot = {
  version: 1;
  profileId: string;
  /** null = 当时没打开任何媒体。 */
  cwd: string | null;
  draft: string;
  turns: ChatTurn[];
  updatedAtMs: number;
};

export type ChatRestoreInput = {
  profileId: string;
  cwd: string | null;
  draft: string;
  turns: readonly ChatTurn[];
};

/** 是否值得落盘：空 turns + 空草稿就不占坑（新建对话后清掉旧快照）。 */
export function isRestorable(input: ChatRestoreInput): boolean {
  return (
    input.draft.trim().length > 0 || pruneChatTurns(input.turns).length > 0
  );
}

function truncateText(value: string): string {
  return value.length > MAX_TEXT_LENGTH
    ? value.slice(0, MAX_TEXT_LENGTH)
    : value;
}

/**
 * 裁剪 turns 到可持久化形态。纯函数，可单测。
 * 注意：返回的是浅拷贝的新对象，不动输入。
 */
export function pruneChatTurns(turns: readonly ChatTurn[]): ChatTurn[] {
  const pruned: ChatTurn[] = [];
  for (const turn of turns) {
    if (
      typeof turn?.userText !== "string" ||
      typeof turn?.answer !== "string"
    ) {
      continue;
    }
    const userText = truncateText(turn.userText);
    const answer = truncateText(turn.answer);
    const hasContent =
      userText.trim().length > 0 ||
      answer.trim().length > 0 ||
      turn.status === "error" ||
      turn.annotationProposalSaved === true;
    if (!hasContent) continue;
    // 流式残留不能复活：有字就冻结，无字就丢掉。
    if (turn.status === "streaming") {
      if (answer.trim().length === 0) continue;
    }
    pruned.push({
      ...turn,
      userText,
      answer,
      status: turn.status === "streaming" ? "done" : turn.status,
      activities: Array.isArray(turn.activities)
        ? turn.activities.map((activity) => ({
            id: activity.id,
            kind: activity.kind,
            ...(activity.toolCallId ? { toolCallId: activity.toolCallId } : {}),
            ...(activity.title ? { title: activity.title } : {}),
            ...(activity.status ? { status: activity.status } : {}),
          }))
        : [],
      images: undefined,
      agentDraft: undefined,
      agentSegments: undefined,
      annotationProposal: undefined,
    });
  }
  return pruned.slice(-MAX_TURNS);
}

function toSnapshot(input: ChatRestoreInput): ChatRestoreSnapshot {
  return {
    version: SNAPSHOT_VERSION,
    profileId: input.profileId,
    cwd: input.cwd,
    draft: input.draft.slice(0, MAX_TEXT_LENGTH),
    turns: pruneChatTurns(input.turns),
    updatedAtMs: Date.now(),
  };
}

function writeSnapshot(snapshot: ChatRestoreSnapshot): void {
  let turns = snapshot.turns;
  for (let attempt = 0; attempt < MAX_WRITE_ATTEMPTS; attempt += 1) {
    try {
      window.localStorage.setItem(
        CHAT_RESTORE_KEY,
        JSON.stringify({ ...snapshot, turns }),
      );
      return;
    } catch {
      // 配额炸了就对半砍再试；最后一次失败就放弃（不断旧快照）。
      turns = turns.slice(Math.ceil(turns.length / 2));
      if (turns.length === 0) return;
    }
  }
}

function removeSnapshot(): void {
  try {
    window.localStorage.removeItem(CHAT_RESTORE_KEY);
  } catch {
    // 私有模式等极端环境：清不掉也不影响，内存态本来就是空的。
  }
}

let pendingInput: ChatRestoreInput | null = null;
let pendingRemoval = false;
let timer: ReturnType<typeof setTimeout> | null = null;

function flushPending(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  if (pendingRemoval) {
    pendingRemoval = false;
    pendingInput = null;
    removeSnapshot();
    return;
  }
  const input = pendingInput;
  pendingInput = null;
  if (!input) return;
  writeSnapshot(toSnapshot(input));
}

/**
 * 节流落盘（trailing 1500ms）。传 null 表示“无可存内容”，直接清快照，
 * 避免新建对话后僵尸恢复。
 */
export function schedulePersistChatRestore(
  input: ChatRestoreInput | null,
): void {
  if (typeof window === "undefined") return;
  if (input === null) {
    pendingInput = null;
    pendingRemoval = true;
  } else {
    pendingRemoval = false;
    pendingInput = input;
  }
  if (timer !== null) return;
  timer = setTimeout(flushPending, WRITE_DELAY_MS);
}

/** 卸载/切后台前把 trailing 的一次写完。 */
export function flushChatRestore(): void {
  if (typeof window === "undefined") return;
  flushPending();
}

function isValidTurn(value: unknown): value is ChatTurn {
  if (typeof value !== "object" || value === null) return false;
  const turn = value as Record<string, unknown>;
  return (
    typeof turn.id === "string" &&
    typeof turn.userText === "string" &&
    typeof turn.answer === "string" &&
    (turn.status === "done" ||
      turn.status === "error" ||
      turn.status === "streaming") &&
    Array.isArray(turn.activities)
  );
}

/** 读快照并校验形态；任何非法直接 null（绝不渲染脏数据）。 */
export function readChatRestore(): ChatRestoreSnapshot | null {
  try {
    const raw = window.localStorage.getItem(CHAT_RESTORE_KEY);
    if (!raw) return null;
    const value: unknown = JSON.parse(raw);
    if (typeof value !== "object" || value === null) return null;
    const record = value as Record<string, unknown>;
    if (record.version !== SNAPSHOT_VERSION) return null;
    if (typeof record.profileId !== "string" || !record.profileId) return null;
    if (
      record.cwd !== null &&
      record.cwd !== undefined &&
      typeof record.cwd !== "string"
    ) {
      return null;
    }
    const draft = typeof record.draft === "string" ? record.draft : "";
    const turns = Array.isArray(record.turns)
      ? record.turns.filter(isValidTurn).map((turn) => ({
          ...turn,
          // 读出来也不复活流式态。
          status: turn.status === "streaming" ? ("done" as const) : turn.status,
        }))
      : [];
    if (turns.length === 0 && draft.trim().length === 0) return null;
    return {
      version: SNAPSHOT_VERSION,
      profileId: record.profileId,
      cwd: (record.cwd as string | null | undefined) ?? null,
      draft,
      turns,
      updatedAtMs:
        typeof record.updatedAtMs === "number" ? record.updatedAtMs : 0,
    };
  } catch {
    return null;
  }
}
