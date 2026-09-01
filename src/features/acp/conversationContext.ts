import type { ChatTurn } from "./types";

const MAX_CONTEXT_CHARS = 12_000;

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
      chunks.push(`助手：${turn.answer.trim()}`);
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
