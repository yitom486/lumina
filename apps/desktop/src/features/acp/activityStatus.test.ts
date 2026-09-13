import { describe, expect, it } from "vitest";

import { hasActiveToolActivity, isToolRunning, waitingLabel } from "./activityStatus";
import type { ChatActivity } from "./types";

describe("activityStatus", () => {
  it("detects running tools", () => {
    expect(isToolRunning("running")).toBe(true);
    expect(isToolRunning("completed")).toBe(false);
  });

  it("builds waiting labels from activity kinds", () => {
    const tools: ChatActivity[] = [
      { id: "1", kind: "tool", toolCallId: "a", status: "running" },
    ];
    expect(hasActiveToolActivity(tools)).toBe(true);
    expect(waitingLabel(tools)).toBe("Agent 正在调用工具…");

    const thoughts: ChatActivity[] = [
      { id: "2", kind: "thought", text: "hmm" },
    ];
    expect(waitingLabel(thoughts)).toBe("Agent 正在思考…");
  });
});
