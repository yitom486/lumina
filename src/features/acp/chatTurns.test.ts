import { describe, expect, it } from "vitest";

import { applyAcpEventToTurn, createTurn } from "./chatTurns";

describe("applyAcpEventToTurn", () => {
  it("keeps only final answer when finished", () => {
    const seq = { n: 0 };
    let turn = createTurn(seq, "你好");
    turn = applyAcpEventToTurn(
      turn,
      { type: "agentMessage", text: "流式" },
      "minimal",
    );
    turn = applyAcpEventToTurn(
      turn,
      { type: "agentThought", text: "想一想" },
      "minimal",
    );
    turn = applyAcpEventToTurn(
      turn,
      {
        type: "finished",
        text: "最终答案",
        stopReason: "end_turn",
      },
      "minimal",
    );
    expect(turn.answer).toBe("最终答案");
    expect(turn.status).toBe("done");
    expect(turn.activities).toHaveLength(0);
    expect(turn.showActivities).toBe(false);
  });

  it("keeps activities in verbose mode", () => {
    const seq = { n: 0 };
    let turn = createTurn(seq, "q");
    turn = applyAcpEventToTurn(
      turn,
      { type: "toolCall", toolCallId: "t1", title: "Run" },
      "verbose",
    );
    turn = applyAcpEventToTurn(
      turn,
      { type: "finished", text: "done", stopReason: "end_turn" },
      "verbose",
    );
    expect(turn.activities.length).toBeGreaterThan(0);
    expect(turn.showActivities).toBe(true);
  });
});
