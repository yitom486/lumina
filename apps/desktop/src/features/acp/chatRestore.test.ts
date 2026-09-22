import { describe, expect, it } from "vitest";

import type { ChatTurn } from "./types";
import { isRestorable, pruneChatTurns } from "./chatRestore";

function makeTurn(partial: Partial<ChatTurn> & { id: string }): ChatTurn {
  return {
    userText: "问题",
    answer: "回答",
    status: "done",
    activities: [],
    showActivities: false,
    ...partial,
  };
}

describe("pruneChatTurns", () => {
  it("drops blank turns and freezes leftover streaming answers", () => {
    const turns = pruneChatTurns([
      makeTurn({ id: "blank", userText: "  ", answer: "" }),
      makeTurn({ id: "stream-empty", userText: "问", answer: "", status: "streaming" }),
      makeTurn({ id: "stream-part", userText: "问", answer: "半截", status: "streaming" }),
      makeTurn({ id: "error", userText: "", answer: "", status: "error" }),
    ]);
    expect(turns.map((turn) => turn.id)).toEqual(["stream-part", "error"]);
    expect(turns[0]?.status).toBe("done");
  });

  it("strips heavy and transient fields without touching the input", () => {
    const input = [
      makeTurn({
        id: "t1",
        images: [{ id: "img", mimeType: "image/png", dataUrl: "data:xxx" }],
        agentDraft: "草稿",
        agentSegments: ["a"],
        activities: [
          { id: "a1", kind: "tool", title: "读库", text: "C:\\secret\\p", status: "done" },
        ],
      }),
    ];
    const [pruned] = pruneChatTurns(input);
    expect(pruned?.images).toBeUndefined();
    expect(pruned?.agentDraft).toBeUndefined();
    expect(pruned?.agentSegments).toBeUndefined();
    expect(pruned?.activities).toEqual([
      { id: "a1", kind: "tool", title: "读库", status: "done" },
    ]);
    expect(input[0]?.images).toHaveLength(1);
  });

  it("keeps only the last 40 turns", () => {
    const turns = Array.from({ length: 50 }, (_, index) =>
      makeTurn({ id: `t${index}`, userText: `q${index}` }),
    );
    const pruned = pruneChatTurns(turns);
    expect(pruned).toHaveLength(40);
    expect(pruned[0]?.id).toBe("t10");
  });

  it("treats empty turns and blank drafts as nothing worth restoring", () => {
    expect(
      isRestorable({ profileId: "codex", cwd: null, draft: "  ", turns: [] }),
    ).toBe(false);
    expect(
      isRestorable({
        profileId: "codex",
        cwd: null,
        draft: "",
        turns: [makeTurn({ id: "t1" })],
      }),
    ).toBe(true);
    expect(
      isRestorable({
        profileId: "codex",
        cwd: null,
        draft: "未发完",
        turns: [],
      }),
    ).toBe(true);
  });
});
