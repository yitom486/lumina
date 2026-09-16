import type { SavedChatConversation } from "./chatHistoryStore";
import type { AgentSessionInfo, SavedSessionHint } from "./types";

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

export type ReconciledChatConversation = SavedChatConversation & {
  agentStatus: AgentSessionStatus;
};

/**
 * Decide whether a list result is safe to use as an allowlist verification.
 * Missing data, a partial page walk, an unverified result, and a busy Agent
 * all deliberately degrade to the local-record view instead of marking ids
 * missing.
 */
export function canUseVerifiedAgentSessions(input: {
  hasData: boolean;
  verified: boolean;
  truncated: boolean;
  busy: boolean;
}): boolean {
  return input.hasData && input.verified && !input.truncated && !input.busy;
}

/** Fetch the metadata once when the user opens history on a connected Agent. */
export function shouldRequestHistorySessionList(input: {
  historyOpen: boolean;
  connected: boolean;
  busy: boolean;
  requested: boolean;
}): boolean {
  return input.historyOpen && input.connected && !input.busy && !input.requested;
}

/**
 * Reconcile the local conversation allowlist with Agent metadata. Agent
 * sessions not present in `local` are intentionally discarded, so unrelated
 * Agent conversations never enter the history UI.
 */
export function reconcileConversations(
  local: SavedChatConversation[],
  agentSessions: AgentSessionInfo[] | null | undefined,
  options: {
    verified: boolean;
    queryScope?: { profileId: string; cwd: string | null };
  },
): ReconciledChatConversation[] {
  const canVerify = options.verified && Array.isArray(agentSessions);
  const byId = new Map(
    (agentSessions ?? []).map((session) => [session.sessionId, session]),
  );

  return local.map((conversation) => {
    const withinQueryScope =
      !options.queryScope ||
      (conversation.profileId === options.queryScope.profileId &&
        conversation.cwd === options.queryScope.cwd);
    const canVerifyConversation = canVerify && withinQueryScope;
    const agent = canVerifyConversation && conversation.agentSessionId
      ? byId.get(conversation.agentSessionId)
      : undefined;
    const agentTitle = agent?.title?.trim();
    const parsedUpdatedAt = agent?.updatedAt
      ? Date.parse(agent.updatedAt)
      : Number.NaN;
    const updatedAtMs = Number.isFinite(parsedUpdatedAt)
      ? parsedUpdatedAt
      : conversation.updatedAtMs;

    return {
      ...conversation,
      agentStatus: !canVerifyConversation
        ? "unverified"
        : agent
          ? "live"
          : "missing",
      title: agentTitle || conversation.title,
      updatedAtMs,
    };
  });
}
