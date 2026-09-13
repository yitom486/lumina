import { isToolSucceeded } from "@/features/acp/toolStatus";
import type { AcpEvent, ChatActivity, ChatTurn } from "@/features/acp/types";

export const PROPOSE_ANNOTATION_TOOL = "lumina_propose_video_annotation";

export function isProposeAnnotationTool(title?: string | null): boolean {
  if (!title?.trim()) return false;
  return title.includes(PROPOSE_ANNOTATION_TOOL);
}

function activityTitleForTool(
  activities: ChatActivity[] | undefined,
  toolCallId: string,
): string | undefined {
  return activities?.find(
    (activity) => activity.kind === "tool" && activity.toolCallId === toolCallId,
  )?.title;
}

export function shouldTrackProposeToolCall(event: AcpEvent): boolean {
  return event.type === "toolCall" && isProposeAnnotationTool(event.title);
}

export function shouldFetchAnnotationProposal(
  event: AcpEvent,
  options?: {
    trackedProposeToolCallIds?: ReadonlySet<string>;
    activities?: ChatActivity[];
  },
): boolean {
  if (event.type !== "toolCallUpdate") return false;
  if (!isToolSucceeded(event.status ?? undefined)) return false;
  if (isProposeAnnotationTool(event.title)) return true;
  if (options?.trackedProposeToolCallIds?.has(event.toolCallId)) return true;
  const activityTitle = activityTitleForTool(
    options?.activities,
    event.toolCallId,
  );
  return isProposeAnnotationTool(activityTitle);
}

export function turnNeedsProposalFetch(turn: ChatTurn): boolean {
  if (turn.annotationProposal || turn.annotationProposalSaved) return false;
  return turn.activities.some(
    (activity) =>
      activity.kind === "tool" &&
      isProposeAnnotationTool(activity.title) &&
      isToolSucceeded(activity.status),
  );
}

/** Fallback rescan only for the latest turn — never attach a new proposal to older turns. */
export function shouldRescanTurnForProposal(
  turn: ChatTurn,
  turns: ChatTurn[],
): boolean {
  if (turn.status !== "done" || !turnNeedsProposalFetch(turn)) return false;
  const lastTurn = turns[turns.length - 1];
  return lastTurn?.id === turn.id;
}
