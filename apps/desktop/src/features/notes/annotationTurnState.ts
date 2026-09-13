import type { ChatTurn } from "@/features/acp/types";

import type { VideoAnnotationProposal } from "./proposalTypes";

export type ProposalBindings = {
  handledProposalIds: Set<string>;
  proposalTurnById: Map<string, string>;
};

export function createProposalBindings(): ProposalBindings {
  return {
    handledProposalIds: new Set<string>(),
    proposalTurnById: new Map<string, string>(),
  };
}

export function seedProposalBindingsFromTurns(
  turns: ChatTurn[],
): ProposalBindings {
  const bindings = createProposalBindings();
  for (const turn of turns) {
    const proposalId = turn.annotationProposal?.proposalId;
    if (!proposalId) continue;
    bindings.handledProposalIds.add(proposalId);
    bindings.proposalTurnById.set(proposalId, turn.id);
  }
  return bindings;
}

/** Whether a loaded proposal may attach to this turn without stealing from another. */
export function canAttachProposalToTurn(
  proposalId: string,
  turnId: string,
  bindings: ProposalBindings,
): boolean {
  const boundTurnId = bindings.proposalTurnById.get(proposalId);
  if (boundTurnId && boundTurnId !== turnId) return false;
  if (
    bindings.handledProposalIds.has(proposalId) &&
    boundTurnId !== turnId
  ) {
    return false;
  }
  return true;
}

export function attachProposalBinding(
  proposalId: string,
  turnId: string,
  bindings: ProposalBindings,
): void {
  bindings.handledProposalIds.add(proposalId);
  bindings.proposalTurnById.set(proposalId, turnId);
}

export function applyAttachAnnotationProposal(
  turns: ChatTurn[],
  turnId: string,
  proposal: VideoAnnotationProposal,
): ChatTurn[] {
  return turns.map((turn) =>
    turn.id === turnId ? { ...turn, annotationProposal: proposal } : turn,
  );
}

export function applyClearTurnAnnotation(
  turns: ChatTurn[],
  turnId: string,
): ChatTurn[] {
  return turns.map((turn) =>
    turn.id === turnId ? { ...turn, annotationProposal: undefined } : turn,
  );
}

/** Mark one turn saved; never mutate other turns' annotation state. */
export function applySaveAnnotation(
  turns: ChatTurn[],
  turnId: string,
  proposalId?: string,
): ChatTurn[] {
  return turns.map((turn) => {
    if (turn.id !== turnId) return turn;
    if (
      proposalId &&
      turn.annotationProposal &&
      turn.annotationProposal.proposalId !== proposalId
    ) {
      return turn;
    }
    return {
      ...turn,
      annotationProposal: undefined,
      annotationProposalSaved: true,
    };
  });
}
