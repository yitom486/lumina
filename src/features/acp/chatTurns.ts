import type { AcpEvent, ChatActivity, ChatTurn, ThinkingLevel } from "./types";
import { hintForAcpFailure } from "./failureHints";
import { mergeToolDetail } from "./toolStatus";

function nextSeq(seq: { n: number }): string {
  seq.n += 1;
  return String(seq.n);
}

export function createTurn(seq: { n: number }, userText: string): ChatTurn {
  const id = `turn-${nextSeq(seq)}`;
  return {
    id,
    userText,
    answer: "",
    status: "streaming",
    activities: [],
    showActivities: true,
  };
}

export function applyAcpEventToTurn(
  turn: ChatTurn,
  event: AcpEvent,
  thinkingLevel: ThinkingLevel,
): ChatTurn {
  switch (event.type) {
    case "agentMessage":
      return {
        ...turn,
        answer: `${turn.answer}${event.text}`,
        status: "streaming",
      };
    case "agentThought":
      if (thinkingLevel === "hidden") return turn;
      return {
        ...turn,
        activities: appendThought(turn.activities, event.text),
      };
    case "toolCall":
      return {
        ...turn,
        activities: upsertTool(turn.activities, {
          id: `tool-${event.toolCallId}`,
          kind: "tool",
          toolCallId: event.toolCallId,
          title: event.title ?? event.toolCallId,
          status: event.status ?? "pending",
          text: mergeToolDetail(undefined, event.detail ?? undefined, false),
        }),
      };
    case "toolCallUpdate":
      return {
        ...turn,
        activities: upsertTool(turn.activities, {
          id: `tool-${event.toolCallId}`,
          kind: "tool",
          toolCallId: event.toolCallId,
          title: event.title ?? undefined,
          status: event.status ?? undefined,
          text: mergeToolDetail(
            turn.activities.find(
              (activity) =>
                activity.kind === "tool" && activity.toolCallId === event.toolCallId,
            )?.text,
            event.detail ?? undefined,
            event.appendDetail ?? false,
          ),
        }),
      };
    case "plan":
      if (thinkingLevel === "hidden") return turn;
      return {
        ...turn,
        activities: [
          ...turn.activities.filter((a) => a.kind !== "plan"),
          {
            id: `plan-${turn.id}`,
            kind: "plan",
            text: event.text,
          },
        ],
      };
    case "finished": {
      const finalText = event.text.trim() || turn.answer.trim();
      const hasActivities = turn.activities.length > 0;
      return {
        ...turn,
        answer: finalText,
        status: "done",
        showActivities: thinkingLevel === "verbose" && hasActivities,
        activities: hasActivities ? turn.activities : [],
      };
    }
    case "failed":
      return {
        ...turn,
        answer: event.message,
        errorHint: hintForAcpFailure(event.code),
        status: "error",
        showActivities: false,
        activities: [],
      };
    default:
      return turn;
  }
}

function appendThought(activities: ChatActivity[], chunk: string): ChatActivity[] {
  const last = activities[activities.length - 1];
  if (last?.kind === "thought") {
    return [
      ...activities.slice(0, -1),
      { ...last, text: `${last.text ?? ""}${chunk}` },
    ];
  }
  return [
    ...activities,
    { id: `thought-${activities.length}`, kind: "thought", text: chunk },
  ];
}

function upsertTool(activities: ChatActivity[], item: ChatActivity): ChatActivity[] {
  const idx = activities.findIndex(
    (a) => a.kind === "tool" && a.toolCallId === item.toolCallId,
  );
  if (idx >= 0) {
    const next = [...activities];
    const previous = next[idx];
    next[idx] = {
      ...previous,
      ...item,
      title: item.title ?? previous.title,
      status: item.status ?? previous.status,
      text: item.text ?? previous.text,
    };
    return next;
  }
  return [...activities, item];
}

export type SystemNotice = { id: string; content: string };

export function pushNotice(
  notices: SystemNotice[],
  seq: { n: number },
  content: string,
): SystemNotice[] {
  return [...notices, { id: `sys-${nextSeq(seq)}`, content }];
}
