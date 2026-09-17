import { describe, expect, it } from "vitest";

import {
  MISSING_HISTORY_CONVERSATION_TITLE,
  isHistoryConversationDisabled,
} from "./conversationContext";

describe("isHistoryConversationDisabled", () => {
  it("disables a proven-missing row with an AI session", () => {
    expect(
      isHistoryConversationDisabled({
        agentStatus: "missing",
        agentSessionId: "agent-1",
      }),
    ).toBe(true);
  });

  it("disables agent-only missing rows the same way", () => {
    // Agent-only 条目一定带 sessionId（origin 不参与判定），同样不可点；
    // 删除按钮本来就不展示，保持现有规则。
    expect(
      isHistoryConversationDisabled({
        agentStatus: "missing",
        agentSessionId: "agent-only",
      }),
    ).toBe(true);
  });

  it("keeps live rows selectable", () => {
    expect(
      isHistoryConversationDisabled({
        agentStatus: "live",
        agentSessionId: "agent-1",
      }),
    ).toBe(false);
  });

  it("keeps unverified rows selectable when the list was truncated", () => {
    expect(
      isHistoryConversationDisabled({
        agentStatus: "unverified",
        agentSessionId: "agent-1",
      }),
    ).toBe(false);
  });

  it("keeps local archive rows without a session id selectable", () => {
    // 本地存档行是合法的本地查看 + F3 新开，判不准也不能禁用。
    expect(
      isHistoryConversationDisabled({
        agentStatus: "missing",
        agentSessionId: null,
      }),
    ).toBe(false);
    expect(
      isHistoryConversationDisabled({
        agentStatus: "unverified",
        agentSessionId: null,
      }),
    ).toBe(false);
    expect(
      isHistoryConversationDisabled({ agentStatus: "live", agentSessionId: "" }),
    ).toBe(false);
  });

  it("exposes the exact missing tooltip copy", () => {
    expect(MISSING_HISTORY_CONVERSATION_TITLE).toBe("AI 记忆已不存在");
  });
});
