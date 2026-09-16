import { composeAssistantAnswer } from "./assistantAnswer";
import type { ChatTurn } from "./types";

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
