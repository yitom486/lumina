import { errorMessage } from "@/lib/format";

import { useChatSnapshotStore } from "./chatSnapshotStore";
import {
  chatHintDelete,
  chatHintGet,
  chatHintUpsert,
  chatSnapshotDelete,
  chatSnapshotGet,
  chatSnapshotUpsert,
} from "./chatStoreRepository";
import type { ChatTurn, SavedSessionHint } from "./types";

/**
 * 聊天秒开恢复（第 1 层：本地渲染缓存）+ SQLite 主层（B3）。
 *
 * 对标 Inkdown 的 `zustand/persist` 层：面板挂载瞬间先渲染内存镜像里的
 * turns + 草稿，零网络；原生会话续接是第 2 层（persisted savedSession
 * + connect 时 resume），见 `acpSessionStore` 与 `AcpPanel` 的对账逻辑。
 *
 * B3 SQLite only 说明：
 * - zustand 内存镜像（`useChatSnapshotStore` 按 `(profile, session)` 分键，
 *   hints 复用 `useAcpSessionStore.savedSessions[profile]`）是热路径唯一真相：
 *   首渲染同步读，不等 DB；hydrate hook 在 effect 里异步回填。
 * - 读顺序：内存镜像 → SQLite → 空。`lumina-acp-chat-restore:*` 备层只在
 *   `migrateLegacyChatStoreOnce` 里一次性读取迁移；标记置位后备层不再被
 *   读取也不再被写入（见 `isChatStoreMigrated` 门控）。
 * - 写穿：schedule* 同步写内存镜像 + 按 profile 分槽排队，1500ms trailing
 *   到期后只写 DB（异步）；DB 失败只记日志、内存保留，
 *   失败条目回队等下次 trailing/卸载 flush 重试。
 * - 只存“可重渲染”的内容，不存瞬态：
 * - 流式中的 turn：有 answer 就冻结为 done，没有就整条丢掉；
 * - 粘贴图片 dataUrl、agentDraft/segments、未确认的批注提议、工具原文
 *   全部裁掉（体重 + 可能含本地路径）；
 * - 空 turn 不存，最多保留 40 条；
 * - 写 DB 节流 1500ms trailing。
 */

/**
 * 旧版本单键（v0）与分键备层。B3 起只做一次性迁移读取
 *（`migrateLegacyChatStoreOnce`），标记置位后不再读写。
 */
export const CHAT_RESTORE_KEY = "lumina-acp-chat-restore";

/** 一次性迁移标记：成功落库后写 `1`，脏数据跳过、失败下次重试。用户数据，绝不删除。 */
export const CHAT_STORE_MIGRATED_MARK = "chat_store_migrated";

/**
 * 备层是否已完成一次性迁移。true = SQLite only：读不再回退备层，
 * 写也不再落备层；false = 迁移前，同步读允许回退备层做一次性展示，
 * 随后由 hydrate 回填 DB 收敛。
 */
export function isChatStoreMigrated(): boolean {
  try {
    return window.localStorage.getItem(CHAT_STORE_MIGRATED_MARK) === "1";
  } catch {
    return false;
  }
}

function normalizeProfileKey(profileId: string): string | null {
  const key = profileId.trim();
  return key.length > 0 ? key : null;
}

/**
 * 旧备层分键（仅一次性迁移扫描用）：codex 的旧 turns/草稿只活在
 * `lumina-acp-chat-restore:codex` 下。B3 起新写入不再落到这些键上，
 * profileId 为空直接拒读，绝不落到裸键上。
 */
export function chatRestoreKeyFor(profileId: string): string {
  return `${CHAT_RESTORE_KEY}:${profileId.trim()}`;
}

const SNAPSHOT_VERSION = 1;
const MAX_TURNS = 40;
const MAX_TEXT_LENGTH = 20_000;
const WRITE_DELAY_MS = 1500;

export type ChatRestoreSnapshot = {
  version: 1;
  profileId: string;
  /**
   * 快照归属的原生会话。老备层数据缺该字段时按 `""` 兼容读，
   * 新写入一律带上，避免跨会话复活别人的备层缓存。
   */
  sessionId?: string;
  /** null = 当时没打开任何媒体。 */
  cwd: string | null;
  draft: string;
  turns: ChatTurn[];
  updatedAtMs: number;
};

export type ChatRestoreInput = {
  profileId: string;
  /**
   * 原生会话 id（显式传入优先，缺省用本模块记住的上次落盘会话，
   * 再缺省 `""`）。调用方（落盘 effect）应传
   * `savedSession?.sessionId ?? ""`，与 DB 复合主键对齐。
   */
  sessionId?: string;
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
    ...(typeof input.sessionId === "string" && input.sessionId
      ? { sessionId: input.sessionId }
      : {}),
    cwd: input.cwd,
    draft: input.draft.slice(0, MAX_TEXT_LENGTH),
    turns: pruneChatTurns(input.turns),
    updatedAtMs: Date.now(),
  };
}

function snapshotSessionOf(snapshot: ChatRestoreSnapshot): string {
  return typeof snapshot.sessionId === "string" ? snapshot.sessionId : "";
}

type PendingEntry =
  | { kind: "write"; sessionId: string; snapshot: ChatRestoreSnapshot }
  | { kind: "clear"; sessionId: string };

// 待落盘按 profile 分槽：A→B 快切时，A 的 trailing 写照样落到 A 的 DB 行上，
// 不会被 B 的 pending 顶掉，也不会串到 B 的键里。快照在调度时一次算好，
// 内存镜像 / DB 两处写同一对象，不存在 flush 时刻重算漂移。
const pendingByProfile = new Map<string, PendingEntry>();
// 最近一次落盘的会话归属：clear 没带 sessionId 时用它定位 DB 复合主键。
// 注意这是纯运行时记忆，刷新后从 hint 重建，跨运行未知会话的孤儿行
// 暂不处理（无 list 命令可枚举，属已知迁移债务）。
const lastSessionByProfile = new Map<string, string>();
let timer: ReturnType<typeof setTimeout> | null = null;

type PendingHintOp =
  | { kind: "upsert"; hint: SavedSessionHint }
  | { kind: "clear" };

// hint 写穿重试队：set/clear 后 void 落库，失败只记日志、内存保留，
// 条目留队等下次 trailing/卸载 flush 或迁移时重试。
const pendingHintOps = new Map<string, PendingHintOp>();

function drainPending(): Array<[string, PendingEntry]> {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  const entries = Array.from(pendingByProfile);
  pendingByProfile.clear();
  return entries;
}

function requeueForRetry(profileKey: string, entry: PendingEntry): void {
  // 已有更新的排队（用户又敲字了）绝不覆盖：新状态 supersede 旧状态。
  if (!pendingByProfile.has(profileKey)) {
    pendingByProfile.set(profileKey, entry);
  }
}

async function writeSnapshotToStore(
  profileKey: string,
  entry: Extract<PendingEntry, { kind: "write" }>,
): Promise<void> {
  try {
    await chatSnapshotUpsert({
      profileId: profileKey,
      sessionId: entry.sessionId,
      cwd: entry.snapshot.cwd,
      draft: entry.snapshot.draft,
      turnsJson: JSON.stringify(entry.snapshot.turns),
    });
  } catch (error) {
    requeueForRetry(profileKey, entry);
    console.error("chat snapshot persist failed", errorMessage(error));
  }
}

async function deleteSnapshotFromStore(
  profileKey: string,
  entry: Extract<PendingEntry, { kind: "clear" }>,
): Promise<void> {
  // 清理以写会话为主，兼带空会话行：hint 已提前清空导致 sessionId 为空
  // 时，至少把无归属行删掉，不留僵尸恢复。
  const targets = entry.sessionId
    ? [entry.sessionId, ""]
    : [""];
  for (const sessionId of targets) {
    try {
      await chatSnapshotDelete({ profileId: profileKey, sessionId });
    } catch (error) {
      requeueForRetry(profileKey, entry);
      console.error("chat snapshot clear failed", errorMessage(error));
      return;
    }
  }
}

function ensureFlushTimer(): void {
  if (timer !== null) return;
  timer = setTimeout(() => {
    void flushChatRestore();
  }, WRITE_DELAY_MS);
}

/**
 * 节流落盘（trailing 1.5s）。profile 随 input 走，键天然隔离。
 * 同步写内存镜像（首渲染/切换 reseed 立即可见），到期只写 DB 主层。
 */
export function schedulePersistChatRestore(input: ChatRestoreInput): void {
  if (typeof window === "undefined") return;
  const key = normalizeProfileKey(input.profileId);
  if (!key) return;
  const sessionId =
    input.sessionId ?? lastSessionByProfile.get(key) ?? "";
  lastSessionByProfile.set(key, sessionId);
  const snapshot = toSnapshot({ ...input, profileId: key, sessionId });
  useChatSnapshotStore.getState().setSnapshot(key, sessionId, snapshot);
  pendingByProfile.set(key, { kind: "write", sessionId, snapshot });
  ensureFlushTimer();
}

/**
 * 清掉指定 profile 的快照（新建对话后调用，避免僵尸恢复）。
 * 只清自家键，别家的一律不动。内存镜像同步清，DB 走 trailing。
 */
export function scheduleClearChatRestore(
  profileId: string,
  sessionId?: string,
): void {
  if (typeof window === "undefined") return;
  const key = normalizeProfileKey(profileId);
  if (!key) return;
  const resolved = sessionId ?? lastSessionByProfile.get(key) ?? "";
  useChatSnapshotStore.getState().clearProfile(key);
  pendingByProfile.set(key, { kind: "clear", sessionId: resolved });
  ensureFlushTimer();
}

/**
 * 丢弃指定 profile 的内存态待写/待清（error/disconnected/切换复用）。
 * 只删 pending 槽位 + 快照内存镜像，不调 DB 删除（B2 起即如此，
 * B3 保持原语义：已落定的 DB 行只能由 scheduleClear 删除，避免把
 * 用户上次落定的可恢复内容连带丢掉）：与
 * scheduleClearChatRestore（排队删快照）语义相反。调用后该 profile
 * 下次落盘走重新调度的全新输入，不会被本次丢弃前的旧 trailing 覆盖。
 * 别家槽位一律不动。
 * 注意：正常切换不断旧世界的 trailing（旧键照样落盘），不要在切换路径上
 * 误调本函数丢掉旧世界的草稿；只在 pending 已知过期（报错/断连后的脏计划）
 * 时调用。
 */
export function discardTransientChatState(profileId: string): void {
  if (typeof window === "undefined") return;
  const key = normalizeProfileKey(profileId);
  if (!key) return;
  pendingByProfile.delete(key);
  useChatSnapshotStore.getState().clearProfile(key);
}

/**
 * 卸载/切后台前把 trailing 的写完。两段式：
 * 1. 内存镜像对全部条目同步落定——timer 到期或
 *    卸载路径调用返回前即一致，不受 DB 异步影响；
 * 2. DB 逐个顺序写，失败回队重试，不抛错。
 * B3 起不再写 localStorage 备层。
 * 调用方一律 `void flushChatRestore()`，不得 await 阻塞卸载路径。
 */
export async function flushChatRestore(): Promise<void> {
  if (typeof window === "undefined") return;
  const entries = drainPending();
  for (const [profileKey, entry] of entries) {
    if (entry.kind === "clear") {
      useChatSnapshotStore.getState().clearProfile(profileKey);
    } else {
      useChatSnapshotStore.getState().setSnapshot(
        profileKey,
        entry.sessionId,
        entry.snapshot,
      );
    }
  }
  for (const [profileKey, entry] of entries) {
    if (entry.kind === "clear") {
      await deleteSnapshotFromStore(profileKey, entry);
    } else {
      await writeSnapshotToStore(profileKey, entry);
    }
  }
  await flushHintRetryQueue();
}

/**
 * 仅测试与状态清理使用：倒掉排队 timer、待写槽、会话记忆与 hint 重试队。
 * 生产路径不得调用（会丢掉未落盘的 trailing）。
 */
export function resetChatStoreEphemeralState(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  pendingByProfile.clear();
  lastSessionByProfile.clear();
  pendingHintOps.clear();
  useChatSnapshotStore.getState().clearAll();
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
      ...(typeof record.sessionId === "string" && record.sessionId
        ? { sessionId: record.sessionId }
        : {}),
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
 * 读指定 profile + 会话的备层快照（仅迁移前一次性读取用）。
 * 只认自家键；分键缺失时回退读旧单键，
 * 但单键里的快照 profile 对不上照样拒掉（绝不把 codex 的线程摆进 cursor）。
 * 分键备层若带会话归属且与本次请求不一致，视为别人的缓存直接跳过
 * （老备层无归属字段的按兼容接受一次，由迁移收敛带上归属）。
 */
function readLocalBackupFor(
  profileKey: string,
  sessionKey: string,
): ChatRestoreSnapshot | null {
  const scoped = readRawSnapshot(chatRestoreKeyFor(profileKey));
  if (
    scoped &&
    scoped.profileId === profileKey &&
    (snapshotSessionOf(scoped) === "" ||
      snapshotSessionOf(scoped) === sessionKey)
  ) {
    return scoped;
  }
  if (scoped) return null;
  const legacy = readRawSnapshot(CHAT_RESTORE_KEY);
  if (legacy && legacy.profileId === profileKey) return legacy;
  return null;
}

/**
 * 读指定 profile 的快照（同步，热路径用）。内存镜像优先；
 * 迁移标记置位后（SQLite only）不再回退备层，未迁移前才允许读一次备层
 * 做一次性展示（随后由 hydrate 回填 DB 收敛）。
 * DB 回填由 hydrate hook 在 effect 里异步做，
 * 这里绝不 await，保证首渲染不阻塞。
 */
export function readChatRestore(
  profileId: string,
  sessionId?: string,
): ChatRestoreSnapshot | null {
  if (typeof window === "undefined") return null;
  const key = normalizeProfileKey(profileId);
  if (!key) return null;
  const sessionKey = sessionId ?? lastSessionByProfile.get(key) ?? "";
  const mirrored = useChatSnapshotStore
    .getState()
    .snapshotFor(key, sessionKey);
  if (mirrored) return { ...mirrored, turns: [...mirrored.turns] };
  if (isChatStoreMigrated()) return null;
  return readLocalBackupFor(key, sessionKey);
}

/**
 * 从 DB 主层读单个快照。命中即校验形态（turnsJson 数组 + turn 逐条校验，
 * 脏行按 miss 处理，绝不渲染）；miss/失败一律 null，调用方走备层回退。
 * 纯读，不碰内存镜像与备层，落定由 hydrate hook 在 epoch 守卫内做。
 */
export async function fetchSnapshotFromStore(
  profileId: string,
  sessionId?: string,
): Promise<ChatRestoreSnapshot | null> {
  if (typeof window === "undefined") return null;
  const key = normalizeProfileKey(profileId);
  if (!key) return null;
  const sessionKey = sessionId ?? "";
  let dto: {
    cwd?: unknown;
    draft?: unknown;
    turnsJson?: unknown;
    updatedAtMs?: unknown;
  } | null;
  try {
    dto = await chatSnapshotGet({ profileId: key, sessionId: sessionKey });
  } catch {
    return null;
  }
  if (!dto || typeof dto !== "object") return null;
  const draft = typeof dto.draft === "string" ? dto.draft : "";
  if (
    dto.cwd !== null &&
    dto.cwd !== undefined &&
    typeof dto.cwd !== "string"
  ) {
    return null;
  }
  let parsed: unknown = null;
  try {
    parsed = typeof dto.turnsJson === "string" ? JSON.parse(dto.turnsJson) : null;
  } catch {
    return null;
  }
  if (!Array.isArray(parsed)) return null;
  const turns = parsed.filter(isValidTurn).map((turn) => ({
    ...turn,
    status: turn.status === "streaming" ? ("done" as const) : turn.status,
  }));
  if (turns.length === 0 && draft.trim().length === 0) return null;
  return {
    version: SNAPSHOT_VERSION,
    profileId: key,
    ...(sessionKey ? { sessionId: sessionKey } : {}),
    cwd: (dto.cwd as string | null | undefined) ?? null,
    draft,
    turns,
    updatedAtMs:
      typeof dto.updatedAtMs === "number" ? dto.updatedAtMs : 0,
  };
}

function isValidStoredHint(value: unknown, profileKey: string): value is SavedSessionHint {
  if (typeof value !== "object" || value === null) return false;
  const hint = value as Record<string, unknown>;
  return (
    typeof hint.sessionId === "string" &&
    hint.sessionId.length > 0 &&
    typeof hint.cwd === "string" &&
    (typeof hint.profileId !== "string" || hint.profileId.trim().length > 0) &&
    profileKey.length > 0
  );
}

/**
 * 从 DB 主层读单个 hint。缺失/非法/失败一律 null，调用方保留内存
 * （offline 备）即可，绝不因此清掉可聊状态。
 */
export async function fetchHintFromStore(
  profileId: string,
): Promise<SavedSessionHint | null> {
  if (typeof window === "undefined") return null;
  const key = normalizeProfileKey(profileId);
  if (!key) return null;
  let dto: unknown = null;
  try {
    dto = await chatHintGet({ profileId: key });
  } catch {
    return null;
  }
  if (!isValidStoredHint(dto, key)) return null;
  const record = dto as Record<string, unknown>;
  return {
    sessionId: record.sessionId as string,
    profileId: key,
    cwd: record.cwd as string,
  };
}

async function flushHintOp(profileKey: string): Promise<boolean> {
  const op = pendingHintOps.get(profileKey);
  if (!op) return true;
  try {
    if (op.kind === "upsert") {
      await chatHintUpsert({
        profileId: profileKey,
        sessionId: op.hint.sessionId,
        cwd: op.hint.cwd,
      });
    } else {
      await chatHintDelete({ profileId: profileKey });
    }
  } catch (error) {
    console.error("chat hint persist failed", errorMessage(error));
    return false;
  }
  // 同槽若已有更新的计划（用户又切会话了），绝不误删新计划。
  if (pendingHintOps.get(profileKey) === op) {
    pendingHintOps.delete(profileKey);
  }
  return true;
}

async function flushHintRetryQueue(): Promise<boolean> {
  let ok = true;
  for (const profileKey of Array.from(pendingHintOps.keys())) {
    const done = await flushHintOp(profileKey);
    if (!done) ok = false;
  }
  return ok;
}

/**
 * set hint 后 void 落库。内存（zustand）调用方已同步写好，
 * 这里只负责 DB 写穿：失败仅日志 + 留队，下次 trailing 重试。
 */
export function persistSessionHintFor(
  profileId: string,
  hint: SavedSessionHint,
): void {
  if (typeof window === "undefined") return;
  const key = normalizeProfileKey(profileId);
  if (!key) return;
  if (typeof hint.sessionId !== "string" || !hint.sessionId) return;
  if (typeof hint.cwd !== "string") return;
  pendingHintOps.set(key, {
    kind: "upsert",
    hint: { sessionId: hint.sessionId, profileId: key, cwd: hint.cwd },
  });
  void flushHintOp(key);
}

/**
 * clear hint 后 void 删库。内存调用方已同步清掉，这里只删 DB：
 * 失败仅日志 + 留队，下次 trailing 重试。
 */
export function clearPersistedSessionHintFor(profileId: string): void {
  if (typeof window === "undefined") return;
  const key = normalizeProfileKey(profileId);
  if (!key) return;
  pendingHintOps.set(key, { kind: "clear" });
  void flushHintOp(key);
}

/**
 * 备层 → DB 的收敛回填（迁移前备层命中时调用一次，不阻塞渲染）。
 * 失败仅日志，内存不受影响。
 */
export function backfillSnapshotToStore(
  profileId: string,
  sessionId: string,
  snapshot: ChatRestoreSnapshot,
): void {
  if (typeof window === "undefined") return;
  const key = normalizeProfileKey(profileId);
  if (!key) return;
  void (async () => {
    try {
      await chatSnapshotUpsert({
        profileId: key,
        sessionId,
        cwd: snapshot.cwd,
        draft: snapshot.draft,
        turnsJson: JSON.stringify(snapshot.turns),
      });
    } catch (error) {
      console.error("chat snapshot backfill failed", errorMessage(error));
    }
  })();
}

type PersistedSessionFile = {
  savedSessions?: unknown;
  savedSession?: unknown;
};

/** 读 zustand persist 落盘的 hint 表（含 v0 单槽兼容），脏条目整条跳过。 */
function readPersistedSessionMap(): Record<string, SavedSessionHint> {
  const next: Record<string, SavedSessionHint> = {};
  let raw: string | null = null;
  try {
    raw = window.localStorage.getItem("lumina-acp-session");
  } catch {
    return next;
  }
  if (!raw) return next;
  let value: unknown = null;
  try {
    value = JSON.parse(raw);
  } catch {
    return next;
  }
  if (typeof value !== "object" || value === null) return next;
  const record = value as { state?: unknown };
  const state =
    typeof record.state === "object" && record.state !== null
      ? (record.state as PersistedSessionFile)
      : (value as PersistedSessionFile);
  const table =
    typeof state.savedSessions === "object" && state.savedSessions !== null
      ? (state.savedSessions as Record<string, unknown>)
      : null;
  if (table) {
    for (const [key, hint] of Object.entries(table)) {
      const profileKey = normalizeProfileKey(key);
      if (!profileKey || !isValidStoredHint(hint, profileKey)) continue;
      const record = hint as Record<string, unknown>;
      next[profileKey] = {
        sessionId: record.sessionId as string,
        profileId: profileKey,
        cwd: record.cwd as string,
      };
    }
  }
  // v0 单槽：按自带 profileId 落键；无合法 hint 直接忽略。
  if (
    Object.keys(next).length === 0 &&
    isValidStoredHint(state.savedSession, "x")
  ) {
    const hint = state.savedSession as Record<string, unknown>;
    const profileKey = normalizeProfileKey(String(hint.profileId ?? ""));
    if (profileKey) {
      next[profileKey] = {
        sessionId: hint.sessionId as string,
        profileId: profileKey,
        cwd: hint.cwd as string,
      };
    }
  }
  return next;
}

/**
 * 一次性迁移：DB 就绪后跑一次，把备层恢复键与 persist 会话表校验后
 * 逐个 upsert 进 DB。成功删已迁恢复键 + 写标记；脏数据跳过；
 * 任何 DB 失败直接 `deferred`（不删键、不写标记，下次重试），绝不阻塞启动。
 *
 * `lumina-acp-session` 本体按硬约束保留作离线备（只迁内容不删键）。
 */
export async function migrateLegacyChatStoreOnce(): Promise<
  "migrated" | "already" | "deferred"
> {
  if (typeof window === "undefined") return "deferred";
  try {
    if (window.localStorage.getItem(CHAT_STORE_MIGRATED_MARK) === "1") {
      return "already";
    }
  } catch {
    return "deferred";
  }
  let restoreKeys: string[] = [];
  try {
    const found: string[] = [];
    for (let i = 0; i < window.localStorage.length; i += 1) {
      const storageKey = window.localStorage.key(i);
      if (
        typeof storageKey === "string" &&
        (storageKey === CHAT_RESTORE_KEY ||
          storageKey.startsWith(`${CHAT_RESTORE_KEY}:`))
      ) {
        found.push(storageKey);
      }
    }
    restoreKeys = found;
  } catch {
    return "deferred";
  }
  const hintMap = readPersistedSessionMap();
  const migratedKeys: string[] = [];
  for (const storageKey of restoreKeys) {
    const snapshot = readRawSnapshot(storageKey);
    // 脏数据跳过：不迁、不删，下次读自然拒掉。
    if (!snapshot) continue;
    const profileKey = normalizeProfileKey(snapshot.profileId);
    if (!profileKey) continue;
    if (
      storageKey !== CHAT_RESTORE_KEY &&
      storageKey !== chatRestoreKeyFor(profileKey)
    ) {
      continue;
    }
    const sessionId = hintMap[profileKey]?.sessionId ?? "";
    try {
      await chatSnapshotUpsert({
        profileId: profileKey,
        sessionId,
        cwd: snapshot.cwd,
        draft: snapshot.draft,
        turnsJson: JSON.stringify(snapshot.turns),
      });
    } catch {
      return "deferred";
    }
    migratedKeys.push(storageKey);
  }
  for (const [profileKey, hint] of Object.entries(hintMap)) {
    try {
      await chatHintUpsert({
        profileId: profileKey,
        sessionId: hint.sessionId,
        cwd: hint.cwd,
      });
    } catch {
      return "deferred";
    }
  }
  const hintsFlushed = await flushHintRetryQueue();
  if (!hintsFlushed) return "deferred";
  try {
    for (const storageKey of migratedKeys) {
      window.localStorage.removeItem(storageKey);
    }
    window.localStorage.setItem(CHAT_STORE_MIGRATED_MARK, "1");
  } catch {
    // 备层标记写失败不翻盘：DB 已是真相，下次 hydrate 照读 DB。
  }
  return "migrated";
}
