export type QueuedPrompt = {
  id: string;
  text: string;
  anchorPositionMs: number;
};

export function createQueuedPrompt(
  text: string,
  anchorPositionMs: number,
  id = `q-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
): QueuedPrompt {
  return {
    id,
    text: text.trim(),
    anchorPositionMs,
  };
}

/** Append to the end of the queue (FIFO). */
export function enqueuePrompt(
  queue: QueuedPrompt[],
  item: QueuedPrompt,
): QueuedPrompt[] {
  if (!item.text) return queue;
  return [...queue, item];
}

/**
 * Barge-in: replace the queue with a single next prompt.
 * Caller should also cancel the in-flight turn.
 */
export function bargeInPrompt(
  _queue: QueuedPrompt[],
  item: QueuedPrompt,
): QueuedPrompt[] {
  if (!item.text) return [];
  return [item];
}

export function removeQueuedPrompt(
  queue: QueuedPrompt[],
  id: string,
): QueuedPrompt[] {
  return queue.filter((item) => item.id !== id);
}

export function clearPromptQueue(): QueuedPrompt[] {
  return [];
}

/** Take the head item; returns null if empty. */
export function dequeuePrompt(queue: QueuedPrompt[]): {
  next: QueuedPrompt | null;
  rest: QueuedPrompt[];
} {
  if (queue.length === 0) {
    return { next: null, rest: queue };
  }
  const [next, ...rest] = queue;
  return { next: next ?? null, rest };
}

export function previewQueuedText(text: string, max = 28): string {
  const trimmed = text.trim().replace(/\s+/g, " ");
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max)}…`;
}
