import { describe, expect, it } from "vitest";

import {
  agentConversationId,
  agentSessionListTrust,
  canSwitchHistoryConversation,
  historyConversationAction,
  historyConversationPresentation,
  isAgentConversationId,
  reconcileConversations,
  resumeHintForConversation,
  resumeOutcomeNotice,
  shouldPersistConversationSelection,
  shouldRequestHistorySessionList,
} from "@lumina/chat-ui/conversationContext";
import type { SavedChatConversation } from "@lumina/chat-ui/chatHistoryStore";
import type { AgentSessionInfo } from "@lumina/chat-ui/types";

function conversation(
  agentSessionId: string | null | undefined = "agent-session-1",
): Pick<SavedChatConversation, "agentSessionId" | "profileId" | "cwd"> {
  return {
    agentSessionId,
    profileId: "codex",
    cwd: "D:\\movie",
  };
}

function savedConversation(
  id: string,
  agentSessionId: string | null,
): SavedChatConversation {
  return {
    id,
    title: `本地 ${id}`,
    cwd: "D:\\movie",
    profileId: "codex",
    agentSessionId,
    updatedAtMs: 1_700_000_000_000,
    turns: [],
  };
}

describe("resumeHintForConversation", () => {
  it("returns a hint when profile and cwd match", () => {
    expect(
      resumeHintForConversation(conversation(), {
        profileId: "codex",
        cwd: "D:\\movie",
      }),
    ).toEqual({
      sessionId: "agent-session-1",
      profileId: "codex",
      cwd: "D:\\movie",
    });
  });

  it("rejects a different profile or cwd", () => {
    expect(
      resumeHintForConversation(conversation(), {
        profileId: "claude",
        cwd: "D:\\movie",
      }),
    ).toBeNull();
    expect(
      resumeHintForConversation(conversation(), {
        profileId: "codex",
        cwd: "D:\\other",
      }),
    ).toBeNull();
  });

  it("treats legacy undefined and empty session ids as unavailable", () => {
    const legacyConversation = {
      ...conversation(),
      agentSessionId: undefined,
    } as unknown as Pick<
      SavedChatConversation,
      "agentSessionId" | "profileId" | "cwd"
    >;
    expect(
      resumeHintForConversation(legacyConversation, {
        profileId: "codex",
        cwd: "D:\\movie",
      }),
    ).toBeNull();
    expect(
      resumeHintForConversation(conversation(""), {
        profileId: "codex",
        cwd: "D:\\movie",
      }),
    ).toBeNull();
  });

});

describe("canSwitchHistoryConversation", () => {
  it("allows switching only when idle", () => {
    expect(
      canSwitchHistoryConversation({ busy: false, creatingSession: false }),
    ).toBe(true);
    expect(
      canSwitchHistoryConversation({ busy: true, creatingSession: false }),
    ).toBe(false);
    expect(
      canSwitchHistoryConversation({ busy: false, creatingSession: true }),
    ).toBe(false);
  });
});

describe("Agent-only conversation ids and behavior", () => {
  it("generates and recognizes a non-local display id", () => {
    const id = agentConversationId("agent-session-1");
    expect(id).toBe("agent:agent-session-1");
    expect(isAgentConversationId(id)).toBe(true);
    expect(isAgentConversationId("chat-local-1")).toBe(false);
  });

  it("keeps Agent-only entries out of local selection state", () => {
    expect(shouldPersistConversationSelection("agent")).toBe(false);
    expect(shouldPersistConversationSelection("local")).toBe(true);
    expect(historyConversationPresentation("agent")).toEqual({
      showAgentOnlyLabel: true,
      showTurnCount: false,
      showDelete: false,
    });
  });
});

describe("historyConversationAction", () => {
  it("prioritizes the busy gate and otherwise blocks missing sessions", () => {
    expect(
      historyConversationAction({
        agentStatus: "missing",
        switchBlocked: true,
      }),
    ).toBe("busy");
    expect(
      historyConversationAction({
        agentStatus: "missing",
        switchBlocked: false,
      }),
    ).toBe("missing");
    expect(
      historyConversationAction({
        agentStatus: "unverified",
        switchBlocked: false,
      }),
    ).toBe("available");
  });
});

describe("agentSessionListTrust", () => {
  it("rejects both assertions for missing, unverified, and busy results", () => {
    for (const input of [
      { hasData: false, verified: true, truncated: false, busy: false },
      { hasData: true, verified: false, truncated: false, busy: false },
      { hasData: true, verified: true, truncated: false, busy: true },
    ]) {
      expect(agentSessionListTrust(input)).toEqual({
        canMatch: false,
        canAssertMissing: false,
      });
    }
  });

  it("still trusts positive matches when the page walk was truncated", () => {
    expect(
      agentSessionListTrust({
        hasData: true,
        verified: true,
        truncated: true,
        busy: false,
      }),
    ).toEqual({ canMatch: true, canAssertMissing: false });
  });

  it("trusts both assertions for a complete walk", () => {
    expect(
      agentSessionListTrust({
        hasData: true,
        verified: true,
        truncated: false,
        busy: false,
      }),
    ).toEqual({ canMatch: true, canAssertMissing: true });
  });
});

describe("resumeOutcomeNotice", () => {
  it("never tells the user an occupied conversation is gone", () => {
    const occupied = resumeOutcomeNotice({
      outcome: "occupied",
      sessionMatchedRequest: false,
    });
    expect(occupied).not.toContain("已不存在");
    expect(occupied).toContain("正被其它程序使用");
    // 占用是可逆的，必须给出下一步而不是让用户以为数据丢了。
    expect(occupied).toContain("可重新恢复");
  });

  it("distinguishes resumed, occupied and unavailable", () => {
    const notices = (["resumed", "occupied", "unavailable"] as const).map(
      (outcome) =>
        resumeOutcomeNotice({ outcome, sessionMatchedRequest: false }),
    );
    expect(new Set(notices).size).toBe(3);
    expect(notices[0]).toBe("已恢复该对话的 AI 记忆");
    expect(notices[2]).toBe("该对话的 AI 记忆已不存在，已作为新对话继续");
  });

  it("leaks no implementation detail into any notice", () => {
    for (const outcome of [
      "resumed",
      "occupied",
      "unavailable",
      null,
      undefined,
    ] as const) {
      const notice = resumeOutcomeNotice({
        outcome,
        sessionMatchedRequest: true,
      });
      expect(notice.length).toBeGreaterThan(0);
      for (const banned of [
        "thread",
        "writer",
        "session",
        "codex",
        "resume",
        "JSON",
      ]) {
        expect(notice.toLowerCase()).not.toContain(banned.toLowerCase());
      }
    }
  });

  it("falls back to the id comparison when the backend sends no outcome", () => {
    expect(
      resumeOutcomeNotice({ outcome: null, sessionMatchedRequest: true }),
    ).toBe("已恢复该对话的 AI 记忆");
    expect(
      resumeOutcomeNotice({ outcome: undefined, sessionMatchedRequest: false }),
    ).toBe("该对话的 AI 记忆已不存在，已作为新对话继续");
  });
});

describe("shouldRequestHistorySessionList", () => {
  it("only requests once from an open, connected, idle history panel", () => {
    expect(
      shouldRequestHistorySessionList({
        historyOpen: true,
        connected: true,
        busy: false,
        requested: false,
      }),
    ).toBe(true);
    expect(
      shouldRequestHistorySessionList({
        historyOpen: true,
        connected: false,
        busy: false,
        requested: false,
      }),
    ).toBe(false);
    expect(
      shouldRequestHistorySessionList({
        historyOpen: true,
        connected: true,
        busy: true,
        requested: false,
      }),
    ).toBe(false);
    expect(
      shouldRequestHistorySessionList({
        historyOpen: true,
        connected: true,
        busy: false,
        requested: true,
      }),
    ).toBe(false);
  });
});

const FULL_TRUST = { canMatch: true, canAssertMissing: true };
const NO_TRUST = { canMatch: false, canAssertMissing: false };
/** 会话总量超过翻页上限时的实际形态：能确认命中，不能断言缺失。 */
const TRUNCATED_TRUST = { canMatch: true, canAssertMissing: false };

describe("reconcileConversations", () => {
  const agentSessions: AgentSessionInfo[] = [
    {
      sessionId: "agent-1",
      cwd: "D:\\movie",
      title: "Agent 标题",
      updatedAt: "2026-09-16T10:00:00.000Z",
      kind: "chat",
    },
    {
      sessionId: "agent-2",
      cwd: "D:\\movie",
      title: null,
      updatedAt: null,
      kind: null,
    },
    {
      sessionId: "developer-session",
      cwd: "D:\\movie",
      title: "不应展示",
      updatedAt: null,
      kind: null,
    },
  ];

  it("marks local matches live and the third local record missing", () => {
    const result = reconcileConversations(
      [
        savedConversation("c1", "agent-1"),
        savedConversation("c2", "agent-2"),
        savedConversation("c3", "agent-missing"),
      ],
      agentSessions,
      { trust: FULL_TRUST },
    );
    expect(result.map((item) => item.agentStatus)).toEqual([
      "live",
      "live",
      "missing",
    ]);
    expect(result[0].title).toBe("Agent 标题");
    expect(result[0].origin).toBe("local");
    expect(result[0].updatedAtMs).toBe(Date.parse("2026-09-16T10:00:00.000Z"));
    expect(result[1].title).toBe("本地 c2");
  });

  it("keeps every local record unverified when data is unavailable", () => {
    const result = reconcileConversations(
      [savedConversation("c1", "agent-1"), savedConversation("c2", null)],
      null,
      { trust: NO_TRUST },
    );
    expect(result.map((item) => item.agentStatus)).toEqual([
      "unverified",
      "unverified",
    ]);
  });

  it("drops Agent sessions that are not in the local allowlist", () => {
    const result = reconcileConversations(
      [savedConversation("c1", "agent-1")],
      agentSessions,
      { trust: FULL_TRUST },
    );
    expect(result).toHaveLength(1);
    expect(result.some((item) => item.title === "不应展示")).toBe(false);
  });

  it("adds verified Agent-only sessions only inside the queried scope", () => {
    const result = reconcileConversations(
      [savedConversation("local", "agent-1")],
      [
        agentSessions[0],
        {
          sessionId: "agent-only",
          cwd: "D:\\movie",
          title: "仅 Agent 对话",
          updatedAt: "2026-09-16T11:00:00.000Z",
          kind: null,
        },
      ],
      {
        trust: FULL_TRUST,
        queryScope: { profileId: "codex", cwd: "D:\\movie" },
      },
    );
    expect(result.map((item) => item.origin)).toEqual(["agent", "local"]);
    expect(result[0].id).toBe(agentConversationId("agent-only"));
    expect(result[0].agentStatus).toBe("live");
    expect(result[0].turns).toEqual([]);
    expect(result[0].title).toBe("仅 Agent 对话");
  });

  it("does not synthesize Agent-only rows when data is unverified or unscoped", () => {
    const agentOnly: AgentSessionInfo = {
      sessionId: "agent-only",
      cwd: "D:\\movie",
      title: "仅 Agent 对话",
      updatedAt: "2026-09-16T11:00:00.000Z",
      kind: null,
    };
    expect(
      reconcileConversations([], [agentOnly], {
        trust: NO_TRUST,
        queryScope: { profileId: "codex", cwd: "D:\\movie" },
      }),
    ).toEqual([]);
    expect(
      reconcileConversations([], [agentOnly], { trust: FULL_TRUST }),
    ).toEqual([]);
  });

  it("still lists Agent-only rows when the page walk was truncated", () => {
    const result = reconcileConversations(
      [savedConversation("c-missing", "agent-beyond-the-window")],
      [
        {
          sessionId: "agent-only",
          cwd: "D:\\movie",
          title: "仅 Agent 对话",
          updatedAt: "2026-09-16T11:00:00.000Z",
          kind: null,
        },
      ],
      {
        trust: TRUNCATED_TRUST,
        queryScope: { profileId: "codex", cwd: "D:\\movie" },
      },
    );
    expect(result.map((item) => item.origin)).toEqual(["agent", "local"]);
    // 没翻完就不能断言本地那条不存在，它只能是未校验而非失效。
    expect(result[1]?.agentStatus).toBe("unverified");
  });

  it("falls back to an unnamed title and zero time for malformed metadata", () => {
    const [item] = reconcileConversations(
      [],
      [
        {
          sessionId: "agent-unnamed",
          cwd: "D:\\movie",
          title: null,
          updatedAt: "not-a-date",
          kind: null,
        },
      ],
      {
        trust: FULL_TRUST,
        queryScope: { profileId: "codex", cwd: "D:\\movie" },
      },
    );
    expect(item.origin).toBe("agent");
    expect(item.title).toBe("未命名对话");
    expect(item.updatedAtMs).toBe(0);
  });

  it("keeps records outside the queried cwd unverified", () => {
    const result = reconcileConversations(
      [
        savedConversation("current", "agent-1"),
        {
          ...savedConversation("other-cwd", "agent-2"),
          cwd: "D:\\other-movie",
        },
      ],
      agentSessions,
      {
        trust: FULL_TRUST,
        queryScope: { profileId: "codex", cwd: "D:\\movie" },
      },
    );
    expect(result[0]?.agentStatus).toBe("live");
    expect(result[1]?.agentStatus).toBe("unverified");
    expect(result[2]?.origin).toBe("agent");
    expect(result[2]?.cwd).toBe("D:\\movie");
  });

  it("keeps records outside the queried profile unverified", () => {
    const result = reconcileConversations(
      [
        savedConversation("current", "agent-1"),
        {
          ...savedConversation("other-profile", "agent-2"),
          profileId: "claude",
        },
      ],
      agentSessions,
      {
        trust: FULL_TRUST,
        queryScope: { profileId: "codex", cwd: "D:\\movie" },
      },
    );
    expect(result[0]?.agentStatus).toBe("live");
    expect(result[1]?.agentStatus).toBe("unverified");
    expect(result[2]?.origin).toBe("agent");
    expect(result[2]?.profileId).toBe("codex");
  });
});
