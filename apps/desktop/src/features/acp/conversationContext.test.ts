import { describe, expect, it } from "vitest";

import {
  canSwitchHistoryConversation,
  canUseVerifiedAgentSessions,
  historyConversationAction,
  reconcileConversations,
  resumeHintForConversation,
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

describe("canUseVerifiedAgentSessions", () => {
  it("rejects missing, unverified, partial, and busy results", () => {
    expect(
      canUseVerifiedAgentSessions({
        hasData: false,
        verified: true,
        truncated: false,
        busy: false,
      }),
    ).toBe(false);
    expect(
      canUseVerifiedAgentSessions({
        hasData: true,
        verified: false,
        truncated: false,
        busy: false,
      }),
    ).toBe(false);
    expect(
      canUseVerifiedAgentSessions({
        hasData: true,
        verified: true,
        truncated: true,
        busy: false,
      }),
    ).toBe(false);
    expect(
      canUseVerifiedAgentSessions({
        hasData: true,
        verified: true,
        truncated: false,
        busy: true,
      }),
    ).toBe(false);
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

describe("reconcileConversations", () => {
  const agentSessions: AgentSessionInfo[] = [
    {
      sessionId: "agent-1",
      cwd: "D:\\movie",
      title: "Agent 标题",
      updatedAt: "2026-09-16T10:00:00.000Z",
    },
    {
      sessionId: "agent-2",
      cwd: "D:\\movie",
      title: null,
      updatedAt: null,
    },
    {
      sessionId: "developer-session",
      cwd: "D:\\movie",
      title: "不应展示",
      updatedAt: null,
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
      { verified: true },
    );
    expect(result.map((item) => item.agentStatus)).toEqual([
      "live",
      "live",
      "missing",
    ]);
    expect(result[0].title).toBe("Agent 标题");
    expect(result[0].updatedAtMs).toBe(Date.parse("2026-09-16T10:00:00.000Z"));
    expect(result[1].title).toBe("本地 c2");
  });

  it("keeps every local record unverified when data is unavailable", () => {
    const result = reconcileConversations(
      [savedConversation("c1", "agent-1"), savedConversation("c2", null)],
      null,
      { verified: false },
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
      { verified: true },
    );
    expect(result).toHaveLength(1);
    expect(result.some((item) => item.title === "不应展示")).toBe(false);
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
        verified: true,
        queryScope: { profileId: "codex", cwd: "D:\\movie" },
      },
    );
    expect(result.map((item) => item.agentStatus)).toEqual([
      "live",
      "unverified",
    ]);
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
        verified: true,
        queryScope: { profileId: "codex", cwd: "D:\\movie" },
      },
    );
    expect(result.map((item) => item.agentStatus)).toEqual([
      "live",
      "unverified",
    ]);
  });
});
