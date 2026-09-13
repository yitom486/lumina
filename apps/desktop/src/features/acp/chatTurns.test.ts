import { describe, expect, it } from "vitest";

import { applyAcpEventToTurn, createTurn, syncTurnIdSeq } from "./chatTurns";

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

  it("keeps thoughts in verbose mode", () => {
    const seq = { n: 0 };
    let turn = createTurn(seq, "你好");
    turn = applyAcpEventToTurn(
      turn,
      { type: "agentThought", text: "想一想" },
      "verbose",
    );
    turn = applyAcpEventToTurn(
      turn,
      {
        type: "finished",
        text: "最终答案",
        stopReason: "end_turn",
      },
      "verbose",
    );
    expect(turn.activities).toHaveLength(1);
    expect(turn.activities[0]?.kind).toBe("thought");
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

  it("drops pre-tool agent text and keeps only post-tool answer", () => {
    const seq = { n: 0 };
    let turn = createTurn(seq, "这段讲了什么");
    turn = applyAcpEventToTurn(
      turn,
      {
        type: "agentMessage",
        text: "Natural English: What is this about?\n\n我先查字幕。",
      },
      "minimal",
    );
    turn = applyAcpEventToTurn(
      turn,
      { type: "toolCall", toolCallId: "t1", title: "lumina_get_transcript_window" },
      "minimal",
    );
    expect(turn.answer).toBe("");
    expect(turn.agentDraft).toBe("");
    expect(turn.agentSegments?.[0]).toContain("Natural English");

    turn = applyAcpEventToTurn(
      turn,
      { type: "toolCallUpdate", toolCallId: "t1", status: "completed" },
      "minimal",
    );
    turn = applyAcpEventToTurn(
      turn,
      {
        type: "agentMessage",
        text: "Natural English: Summary\n\n这段主要讲……",
      },
      "minimal",
    );
    expect(turn.answer).toContain("这段主要讲");

    turn = applyAcpEventToTurn(
      turn,
      {
        type: "finished",
        text: "Natural English: Summary\n\n这段主要讲……",
        stopReason: "end_turn",
      },
      "minimal",
    );
    expect(turn.answer).toBe("这段主要讲……");
    expect(turn.answer).not.toMatch(/Natural English/i);
  });
});

describe("syncTurnIdSeq", () => {
  it("bumps the counter above restored history ids", () => {
    const seq = { n: 0 };
    syncTurnIdSeq(seq, [
      {
        id: "turn-3",
        userText: "a",
        answer: "b",
        status: "done",
        activities: [],
        showActivities: false,
      },
      {
        id: "turn-4-1700000000000",
        userText: "c",
        answer: "d",
        status: "done",
        activities: [],
        showActivities: false,
      },
    ]);
    const next = createTurn(seq, "继续");
    expect(next.id).toMatch(/^turn-5-/);
  });
});
