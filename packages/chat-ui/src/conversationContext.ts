import { composeAssistantAnswer } from "./assistantAnswer";
import type { SavedChatConversation } from "./chatHistoryStore";
import type { ChatTurn, SavedSessionHint } from "./types";

const MAX_CONTEXT_CHARS = 12_000;

/**
 * 历史摘要注入状态。恢复历史对话时 Agent 会话是新的、看不到既往 turn，
 * 所以要注入一次；注入成功后该会话自己就攒着上下文，不必每轮重发。
 */
export type HistoryInjectionState = {
  /** 本 chat 存在 Agent 未见过的既往 turn */
  armed: boolean;
  /** 已注入过的 Agent session id；undefined 表示从未注入 */
  injectedSessionId?: string | null;
};

/** 注入过的会话若被换掉（resume 失败、重连），新会话要重新注入一次。 */
export function shouldInjectHistoryContext(
  state: HistoryInjectionState,
  currentSessionId: string | null,
): boolean {
  if (!state.armed) return false;
  if (state.injectedSessionId === undefined) return true;
  return state.injectedSessionId !== currentSessionId;
}

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

/**
 * 能否立刻把会话切到目标对话。只有为 true 时才可以把「已注入」台账记成目标
 * session id；否则必须退回摘要兜底。
 *
 * 正忙时既关不掉当前会话、也不会触发自动连接，而 Rust 侧的 prompt 在已有 live
 * session 时会直接复用它并忽略 hint（`runtime/prompt.rs` 的 `guard.is_none()`
 * 分支）。此时若把台账记成目标 id，下一轮就会既不注入摘要、又跑在另一条会话上。
 */
export function canAdoptResumeTarget(input: {
  /** 目标会话就是当前活跃会话 */
  targetIsLiveSession: boolean;
  sessionActive: boolean;
  /** 正在回答或正在新建会话 */
  transitionBlocked: boolean;
}): boolean {
  if (input.targetIsLiveSession) return true;
  // 没有活跃会话时，自动连接流程会带着 hint 接管。
  if (!input.sessionActive) return true;
  return !input.transitionBlocked;
}

/** Format prior turns for Agent prompt injection (not shown in UI). */
export function formatConversationHistoryContext(
  turns: ChatTurn[],
  excludeTurnId?: string,
): string | null {
  const completed = turns.filter(
    (turn) =>
      turn.id !== excludeTurnId &&
      turn.status !== "streaming" &&
      (turn.userText.trim() || turn.answer.trim()),
  );
  if (completed.length === 0) return null;

  const lines: string[] = [];
  let total = 0;
  for (const turn of completed) {
    const chunks: string[] = [];
    if (turn.userText.trim()) {
      chunks.push(`用户：${turn.userText.trim()}`);
    }
    if (turn.answer.trim()) {
      const answer = composeAssistantAnswer(turn.answer, turn.activities);
      if (answer) {
        chunks.push(`助手：${answer}`);
      }
    }
    const block = chunks.join("\n");
    if (!block) continue;
    if (total + block.length + 2 > MAX_CONTEXT_CHARS) {
      lines.unshift("…（更早的对话已省略）");
      break;
    }
    lines.push(block);
    total += block.length + 2;
  }

  if (lines.length === 0) return null;
  return lines.join("\n\n");
}
