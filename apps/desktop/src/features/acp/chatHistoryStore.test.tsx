import { beforeEach, describe, expect, it } from "vitest";

import type { ChatTurn } from "./types";
import {
  listConversationsForScope,
  useChatHistoryStore,
} from "./chatHistoryStore";

function makeTurn(userText: string): ChatTurn {
  return {
    id: "turn-1",
    userText,
    answer: "回答",
    status: "done",
    activities: [],
    showActivities: false,
  };
}

beforeEach(() => {
  localStorage.clear();
  useChatHistoryStore.setState({
    conversations: [],
    activeConversationId: null,
  });
});

describe("useChatHistoryStore", () => {
  it("persists conversations with title from first user message", () => {
    useChatHistoryStore.getState().upsertActiveConversation({
      id: "chat-1",
      cwd: "D:\\movie",
      profileId: "codex",
      turns: [makeTurn("这本书的写作顺序是什么？")],
    });

    const saved = useChatHistoryStore.getState().conversations;
    expect(saved).toHaveLength(1);
    expect(saved[0]?.title).toBe("这本书的写作顺序是什么？");
    expect(saved[0]?.turns[0]?.answer).toBe("回答");
  });

  it("filters history by video cwd unless includeAll", () => {
    useChatHistoryStore.setState({
      conversations: [
        {
          id: "a",
          title: "A",
          cwd: "D:\\movie\\a",
          profileId: "codex",
          updatedAtMs: 2,
          turns: [makeTurn("a")],
        },
        {
          id: "b",
          title: "B",
          cwd: "D:\\movie\\b",
          profileId: "codex",
          updatedAtMs: 1,
          turns: [makeTurn("b")],
        },
      ],
      activeConversationId: null,
    });

    const scoped = listConversationsForScope(
      useChatHistoryStore.getState().conversations,
      "D:\\movie\\a",
      "codex",
      false,
    );
    expect(scoped.map((item) => item.id)).toEqual(["a"]);

    const all = listConversationsForScope(
      useChatHistoryStore.getState().conversations,
      "D:\\movie\\a",
      "codex",
      true,
    );
    expect(all).toHaveLength(2);
  });
});
