import { describe, expect, it } from "vitest";

import { formatConversationHistoryContext } from "./conversationContext";
import type { ChatTurn } from "./types";

function turn(id: string, userText: string, answer: string): ChatTurn {
  return {
    id,
    userText,
    answer,
    status: "done",
    activities: [],
    showActivities: false,
  };
}

describe("formatConversationHistoryContext", () => {
  it("formats completed turns and excludes streaming turn", () => {
    const context = formatConversationHistoryContext(
      [
        turn("t1", "第一个问题", "第一个回答"),
        { ...turn("t2", "进行中", ""), status: "streaming" },
      ],
      "t2",
    );
    expect(context).toContain("用户：第一个问题");
    expect(context).toContain("助手：第一个回答");
    expect(context).not.toContain("进行中");
  });

  it("returns null when there is no completed turn", () => {
    expect(
      formatConversationHistoryContext([
        { ...turn("t1", "q", ""), status: "streaming" },
      ]),
    ).toBeNull();
  });

  it("strips English translation blocks from injected assistant history", () => {
    const context = formatConversationHistoryContext([
      {
        ...turn(
          "t1",
          "截图工具能用吗",
          "Natural English: Is it available?\n\n目前还不行。",
        ),
        activities: [],
      },
    ]);
    expect(context).toContain("助手：目前还不行。");
    expect(context).not.toMatch(/Natural English/i);
  });
});
