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

/**
 * 旧版本单键（v0）。分键迁移后只读回退：读不到分键快照时，
 * 若单键里的快照恰好属于要读的 profile 才认一次，下次落盘自然迁走。
 */
export const CHAT_RESTORE_KEY = "lumina-acp-chat-restore";

function normalizeProfileKey(profileId: string): string | null {
  const key = profileId.trim();
  return key.length > 0 ? key : null;
}

/**
 * 各 agent 的渲染缓存各睡各的键：codex 的 turns/草稿只活在
 * `lumina-acp-chat-restore:codex` 下，切到 claude/cursor 再切回，
 * 自家快照原样还在。profileId 为空直接拒写，绝不落到裸键上。
 */
export function chatRestoreKeyFor(profileId: string): string {
  return `${CHAT_RESTORE_KEY}:${profileId.trim()}`;
}

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

function writeSnapshot(key: string, snapshot: ChatRestoreSnapshot): void {
  let turns = snapshot.turns;
  for (let attempt = 0; attempt < MAX_WRITE_ATTEMPTS; attempt += 1) {
    try {
      window.localStorage.setItem(key, JSON.stringify({ ...snapshot, turns }));
      return;
    } catch {
      // 配额炸了就对半砍再试；最后一次失败就放弃（不断旧快照）。
      turns = turns.slice(Math.ceil(turns.length / 2));
      if (turns.length === 0) return;
    }
  }
}

function removeSnapshot(key: string): void {
  try {
    window.localStorage.removeItem(key);
  } catch {
    // 私有模式等极端环境：清不掉也不影响，内存态本来就是空的。
  }
}

/**
 * 旧单键收尾：如果它里面还躺着这个 profile 的快照（升级前写的），
 * 分键落定后就删掉，避免僵尸恢复。不是这个 profile 的一律不动。
 */
function removeLegacySnapshotIfOwnedBy(profileKey: string): void {
  let raw: string | null = null;
  try {
    raw = window.localStorage.getItem(CHAT_RESTORE_KEY);
  } catch {
    return;
  }
  if (!raw) return;
  try {
    const value: unknown = JSON.parse(raw);
    if (
      typeof value === "object" &&
      value !== null &&
      (value as Record<string, unknown>).profileId === profileKey
    ) {
      removeSnapshot(CHAT_RESTORE_KEY);
    }
  } catch {
    // 脏单键：下次读自然拒掉，这里不动它。
  }
}

type PendingEntry =
  | { kind: "write"; input: ChatRestoreInput }
  | { kind: "clear" };

// 待落盘按 profile 分槽：A→B 快切时，A 的 trailing 写照样落到 A 的键上，
// 不会被 B 的 pending 顶掉，也不会串到 B 的键里。
const pendingByProfile = new Map<string, PendingEntry>();
let timer: ReturnType<typeof setTimeout> | null = null;

function flushPending(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  for (const [profileKey, entry] of pendingByProfile) {
    pendingByProfile.delete(profileKey);
    if (entry.kind === "clear") {
      removeSnapshot(chatRestoreKeyFor(profileKey));
      removeLegacySnapshotIfOwnedBy(profileKey);
      continue;
    }
    const key = chatRestoreKeyFor(profileKey);
    writeSnapshot(key, toSnapshot(entry.input));
    removeLegacySnapshotIfOwnedBy(profileKey);
  }
}

function ensureFlushTimer(): void {
  if (timer !== null) return;
  timer = setTimeout(flushPending, WRITE_DELAY_MS);
}

/**
 * 节流落盘（trailing 1.5s）。profile 随 input 走，键天然隔离。
 */
export function schedulePersistChatRestore(input: ChatRestoreInput): void {
  if (typeof window === "undefined") return;
  const key = normalizeProfileKey(input.profileId);
  if (!key) return;
  pendingByProfile.set(key, {
    kind: "write",
    input: { ...input, profileId: key },
  });
  ensureFlushTimer();
}

/**
 * 清掉指定 profile 的快照（新建对话后调用，避免僵尸恢复）。
 * 只清自家键，别家的一律不动。
 */
export function scheduleClearChatRestore(profileId: string): void {
  if (typeof window === "undefined") return;
  const key = normalizeProfileKey(profileId);
  if (!key) return;
  pendingByProfile.set(key, { kind: "clear" });
  ensureFlushTimer();
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

/** 读单个键的快照并校验形态；任何非法直接 null（绝不渲染脏数据）。 */
function readRawSnapshot(key: string): ChatRestoreSnapshot | null {
  try {
    const raw = window.localStorage.getItem(key);
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

/**
 * 读指定 profile 的快照。只认自家键；分键缺失时回退读旧单键，
 * 但单键里的快照 profile 对不上照样拒掉（绝不把 codex 的线程摆进 cursor）。
 */
export function readChatRestore(profileId: string): ChatRestoreSnapshot | null {
  if (typeof window === "undefined") return null;
  const key = normalizeProfileKey(profileId);
  if (!key) return null;
  const scoped = readRawSnapshot(chatRestoreKeyFor(key));
  if (scoped && scoped.profileId === key) return scoped;
  if (scoped) return null;
  const legacy = readRawSnapshot(CHAT_RESTORE_KEY);
  if (legacy && legacy.profileId === key) return legacy;
  return null;
}
