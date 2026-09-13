import { describe, expect, it } from "vitest";

import type { ChatTurn } from "@/features/acp/types";

import {
  applyAttachAnnotationProposal,
  applyClearTurnAnnotation,
  applySaveAnnotation,
  attachProposalBinding,
  canAttachProposalToTurn,
  createProposalBindings,
  seedProposalBindingsFromTurns,
} from "./annotationTurnState";
import type { VideoAnnotationProposal } from "./proposalTypes";

function makeProposal(id: string): VideoAnnotationProposal {
  return {
    proposalId: id,
    mediaPath: "D:\\show.mkv",
    positionMs: 90_000,
    body: `批注 ${id}`,
    includeQuotes: false,
    quotes: [],
    previewMarkdown: "### 1:30\n\n批注",
    createdAtMs: 1,
  };
}

function makeProposeTurn(
  id: string,
  overrides?: Partial<ChatTurn>,
): ChatTurn {
  return {
    id,
    userText: "写批注",
    answer: "已生成提议",
    status: "done",
    activities: [
      {
        id: `tool-${id}`,
        kind: "tool",
        toolCallId: id,
        title: "mcp.lumina.lumina_propose_video_annotation",
        status: "completed",
      },
    ],
    showActivities: false,
    ...overrides,
  };
}

describe("annotationTurnState", () => {
  it("saving one turn does not change other turns", () => {
    const turns: ChatTurn[] = [
      makeProposeTurn("turn-a", {
        annotationProposal: makeProposal("proposal-a"),
      }),
      makeProposeTurn("turn-b", {
        annotationProposal: makeProposal("proposal-b"),
      }),
      {
        id: "turn-c",
        userText: "普通提问",
        answer: "普通回答",
        status: "done",
        activities: [],
        showActivities: false,
      },
    ];

    const next = applySaveAnnotation(turns, "turn-b", "proposal-b");

    expect(next[0]).toEqual(turns[0]);
    expect(next[1]?.annotationProposalSaved).toBe(true);
    expect(next[1]?.annotationProposal).toBeUndefined();
    expect(next[2]).toEqual(turns[2]);
  });

  it("already-saved turns stay saved when another turn is saved", () => {
    const turns: ChatTurn[] = [
      makeProposeTurn("turn-a", { annotationProposalSaved: true }),
      makeProposeTurn("turn-b", {
        annotationProposal: makeProposal("proposal-b"),
      }),
    ];

    const next = applySaveAnnotation(turns, "turn-b", "proposal-b");

    expect(next[0]?.annotationProposalSaved).toBe(true);
    expect(next[1]?.annotationProposalSaved).toBe(true);
  });

  it("attaching a proposal only updates the target turn", () => {
    const turns: ChatTurn[] = [
      makeProposeTurn("turn-a", { annotationProposalSaved: true }),
      makeProposeTurn("turn-b"),
    ];
    const proposal = makeProposal("proposal-b");

    const next = applyAttachAnnotationProposal(turns, "turn-b", proposal);

    expect(next[0]).toEqual(turns[0]);
    expect(next[1]?.annotationProposal).toEqual(proposal);
  });

  it("clearing annotation only affects the target turn", () => {
    const turns: ChatTurn[] = [
      makeProposeTurn("turn-a", {
        annotationProposal: makeProposal("proposal-a"),
      }),
      makeProposeTurn("turn-b", {
        annotationProposal: makeProposal("proposal-b"),
      }),
    ];

    const next = applyClearTurnAnnotation(turns, "turn-b");

    expect(next[0]?.annotationProposal?.proposalId).toBe("proposal-a");
    expect(next[1]?.annotationProposal).toBeUndefined();
  });

  it("rejects attaching a proposal already bound to another turn", () => {
    const bindings = createProposalBindings();
    attachProposalBinding("proposal-x", "turn-a", bindings);

    expect(canAttachProposalToTurn("proposal-x", "turn-b", bindings)).toBe(
      false,
    );
    expect(canAttachProposalToTurn("proposal-x", "turn-a", bindings)).toBe(
      true,
    );
  });

  it("allows attaching a fresh proposal to the latest turn", () => {
    const bindings = createProposalBindings();

    expect(canAttachProposalToTurn("proposal-new", "turn-b", bindings)).toBe(
      true,
    );
  });

  it("seeds bindings from restored turns without touching turn objects", () => {
    const turns: ChatTurn[] = [
      makeProposeTurn("turn-a", {
        annotationProposal: makeProposal("proposal-a"),
      }),
      makeProposeTurn("turn-b", { annotationProposalSaved: true }),
    ];

    const bindings = seedProposalBindingsFromTurns(turns);

    expect(bindings.handledProposalIds.has("proposal-a")).toBe(true);
    expect(bindings.proposalTurnById.get("proposal-a")).toBe("turn-a");
    expect(bindings.handledProposalIds.size).toBe(1);
    expect(turns[0]?.annotationProposal?.proposalId).toBe("proposal-a");
  });

  it("ignores save when proposalId does not match the turn card", () => {
    const turns: ChatTurn[] = [
      makeProposeTurn("turn-a", {
        annotationProposal: makeProposal("proposal-a"),
      }),
    ];

    const next = applySaveAnnotation(turns, "turn-a", "proposal-other");

    expect(next[0]).toEqual(turns[0]);
    expect(next[0]?.annotationProposalSaved).toBeUndefined();
  });
});
