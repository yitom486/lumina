import type { SavedChatConversation } from "./chatHistoryStore";
import type {
  AgentSessionInfo,
  ResumeOutcome,
  SavedSessionHint,
} from "./types";

/**
 * Build a resume hint only when the saved conversation belongs to the exact
 * profile and workspace currently being connected to.
 *
 * `cwd: null` cannot produce a valid SavedSessionHint: the ACP lifecycle has
 * already resolved a concrete workspace before comparing it for resume.
 */
export function resumeHintForConversation(
  conversation: Pick<
    SavedChatConversation,
    "agentSessionId" | "profileId" | "cwd"
  >,
  scope: { profileId: string; cwd: string | null },
): SavedSessionHint | null {
  const sessionId = conversation.agentSessionId;
  if (!sessionId || !conversation.cwd || !scope.cwd) return null;
  if (conversation.profileId !== scope.profileId) return null;
  if (conversation.cwd !== scope.cwd) return null;

  return {
    sessionId,
    profileId: scope.profileId,
    cwd: scope.cwd,
  };
}

/** Pure gate shared by the history list and the load path. */
export function canSwitchHistoryConversation(input: {
  busy: boolean;
  creatingSession: boolean;
}): boolean {
  return !input.busy && !input.creatingSession;
}

export type HistoryConversationAction = "available" | "busy" | "missing";

/** Resolve the UI action state without embedding the priority in a component. */
export function historyConversationAction(input: {
  agentStatus: AgentSessionStatus;
  switchBlocked: boolean;
}): HistoryConversationAction {
  if (input.switchBlocked) return "busy";
  if (input.agentStatus === "missing") return "missing";
  return "available";
}

export type AgentSessionStatus = "live" | "missing" | "unverified";
export type ConversationOrigin = "local" | "agent";

export type HistoryConversationPresentation = {
  showAgentOnlyLabel: boolean;
  showTurnCount: boolean;
  showDelete: boolean;
};

export function historyConversationPresentation(
  origin: ConversationOrigin,
): HistoryConversationPresentation {
  return origin === "agent"
    ? { showAgentOnlyLabel: true, showTurnCount: false, showDelete: false }
    : { showAgentOnlyLabel: false, showTurnCount: true, showDelete: true };
}

export function shouldPersistConversationSelection(
  origin: ConversationOrigin,
): boolean {
  return origin === "local";
}

export type ReconciledChatConversation = SavedChatConversation & {
  agentStatus: AgentSessionStatus;
  origin: ConversationOrigin;
};

const AGENT_CONVERSATION_ID_PREFIX = "agent:";

/** Stable display id for an Agent session that has no local transcript. */
export function agentConversationId(sessionId: string): string {
  return `${AGENT_CONVERSATION_ID_PREFIX}${sessionId}`;
}

/** Identify synthetic Agent-only history entries without touching persistence. */
export function isAgentConversationId(id: string): boolean {
  return id.startsWith(AGENT_CONVERSATION_ID_PREFIX);
}

/**
 * 列表结果能支撑哪种断言。分两层，因为「没翻完」只影响反向判断：
 * 已经出现在返回里的会话是确实存在的正向证据，与翻页是否穷尽无关；
 * 而「没出现」只有在翻完之后才能解释成不存在。
 *
 * 曾经把这两者合成一个布尔，导致会话总量超过翻页上限时整份结果被否决，
 * 历史面板什么都显示不出来。
 */
export type AgentSessionListTrust = {
  /** 命中可断言存在，可据此标 live 并合成 Agent-only 条目 */
  canMatch: boolean;
  /** 未命中可断言不存在，可据此标 missing */
  canAssertMissing: boolean;
};

export function agentSessionListTrust(input: {
  hasData: boolean;
  verified: boolean;
  truncated: boolean;
  busy: boolean;
}): AgentSessionListTrust {
  const canMatch = input.hasData && input.verified && !input.busy;
  return { canMatch, canAssertMissing: canMatch && !input.truncated };
}

/** Fetch the metadata once when the user opens history on a connected Agent. */
/**
 * 恢复 AI 记忆后给用户看的提示。
 *
 * 「被占用」和「已不存在」必须是两句话：占用方（通常是另一个正在运行的 AI
 * 客户端）放手后那条对话还能恢复，统一说成「已不存在」等于谎报数据丢失。
 *
 * `outcome` 缺省时退回旧的 id 比对信号，这样后端还没带上结果也不会静默。
 */
export function resumeOutcomeNotice(input: {
  outcome: ResumeOutcome | null | undefined;
  sessionMatchedRequest: boolean;
}): string {
  switch (input.outcome) {
    case "resumed":
      return "已恢复该对话的 AI 记忆";
    case "occupied":
      return "该对话的 AI 记忆正被其它程序使用，已作为新对话继续；关闭其它 AI 客户端后可重新恢复";
    case "unavailable":
      return "该对话的 AI 记忆已不存在，已作为新对话继续";
    default:
      return input.sessionMatchedRequest
        ? "已恢复该对话的 AI 记忆"
        : "该对话的 AI 记忆已不存在，已作为新对话继续";
  }
}

export function shouldRequestHistorySessionList(input: {
  historyOpen: boolean;
  connected: boolean;
  busy: boolean;
  requested: boolean;
}): boolean {
  return input.historyOpen && input.connected && !input.busy && !input.requested;
}

/**
 * Reconcile local records with Agent metadata. Only a verified result scoped
 * to the exact queried cwd may add Agent-only entries; unverified data never
 * invents history rows, and the Agent response is still bounded by that cwd.
 */
export function reconcileConversations(
  local: SavedChatConversation[],
  agentSessions: AgentSessionInfo[] | null | undefined,
  options: {
    trust: AgentSessionListTrust;
    queryScope?: { profileId: string; cwd: string | null };
  },
): ReconciledChatConversation[] {
  const canMatch = options.trust.canMatch && Array.isArray(agentSessions);
  const byId = new Map(
    (agentSessions ?? []).map((session) => [session.sessionId, session]),
  );

  const reconciledLocal = local.map((conversation) => {
    const withinQueryScope =
      !options.queryScope ||
      (conversation.profileId === options.queryScope.profileId &&
        conversation.cwd === options.queryScope.cwd);
    const matchable = canMatch && withinQueryScope;
    const agent = matchable && conversation.agentSessionId
      ? byId.get(conversation.agentSessionId)
      : undefined;
    const agentTitle = agent?.title?.trim();
    const parsedUpdatedAt = agent?.updatedAt
      ? Date.parse(agent.updatedAt)
      : Number.NaN;
    const updatedAtMs = Number.isFinite(parsedUpdatedAt)
      ? parsedUpdatedAt
      : conversation.updatedAtMs;
    const agentStatus: AgentSessionStatus = agent
      ? "live"
      : matchable && options.trust.canAssertMissing
        ? "missing"
        : "unverified";

    return {
      ...conversation,
      agentStatus,
      origin: "local" as const,
      title: agentTitle || conversation.title,
      updatedAtMs,
    };
  });

  if (!canMatch || !options.queryScope || !options.queryScope.cwd) {
    return reconciledLocal;
  }

  const localSessionIds = new Set(
    local
      .map((conversation) => conversation.agentSessionId)
      .filter((sessionId): sessionId is string => Boolean(sessionId)),
  );
  const agentOnly = (agentSessions ?? [])
    .filter(
      (session) =>
        session.cwd === options.queryScope?.cwd &&
        !localSessionIds.has(session.sessionId),
    )
    .map((session) => {
      const parsedUpdatedAt = session.updatedAt
        ? Date.parse(session.updatedAt)
        : Number.NaN;
      return {
        id: agentConversationId(session.sessionId),
        title: session.title?.trim() || "未命名对话",
        cwd: options.queryScope!.cwd,
        profileId: options.queryScope!.profileId,
        agentSessionId: session.sessionId,
        updatedAtMs: Number.isFinite(parsedUpdatedAt) ? parsedUpdatedAt : 0,
        turns: [],
        agentStatus: "live" as const,
        origin: "agent" as const,
      };
    });

  return [...reconciledLocal, ...agentOnly].sort(
    (a, b) => b.updatedAtMs - a.updatedAtMs,
  );
}
