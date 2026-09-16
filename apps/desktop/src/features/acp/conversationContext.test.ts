import { describe, expect, it } from "vitest";

import {
  canAdoptResumeTarget,
  formatConversationHistoryContext,
  resumeHintForConversation,
  shouldInjectHistoryContext,
} from "@lumina/chat-ui/conversationContext";
import type { SavedChatConversation } from "@lumina/chat-ui/chatHistoryStore";
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

function conversation(
  agentSessionId: string | null | undefined = "agent-session-1",
): Pick<SavedChatConversation, "agentSessionId" | "profileId" | "cwd"> {
  return {
    agentSessionId,
    profileId: "codex",
    cwd: "D:\\movie",
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

describe("shouldInjectHistoryContext", () => {
  it("does not inject for a fresh chat", () => {
    expect(shouldInjectHistoryContext({ armed: false }, "s1")).toBe(false);
  });

  it("injects once after restoring a conversation", () => {
    expect(shouldInjectHistoryContext({ armed: true }, "s1")).toBe(true);
  });

  it("stops injecting on later turns of the same session", () => {
    expect(
      shouldInjectHistoryContext({ armed: true, injectedSessionId: "s1" }, "s1"),
    ).toBe(false);
  });

  it("re-injects once when the agent session is replaced", () => {
    expect(
      shouldInjectHistoryContext({ armed: true, injectedSessionId: "s1" }, "s2"),
    ).toBe(true);
  });

  it("treats a still-unknown session id as already injected", () => {
    expect(
      shouldInjectHistoryContext({ armed: true, injectedSessionId: null }, null),
    ).toBe(false);
  });
});

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

  it("does not inject after a successful resume", () => {
    const hint = resumeHintForConversation(conversation(), {
      profileId: "codex",
      cwd: "D:\\movie",
    });
    expect(hint).not.toBeNull();
    expect(
      shouldInjectHistoryContext(
        { armed: true, injectedSessionId: hint?.sessionId },
        hint?.sessionId ?? null,
      ),
    ).toBe(false);
  });

  it("injects once after resume falls back to a new session", () => {
    const hint = resumeHintForConversation(conversation(), {
      profileId: "codex",
      cwd: "D:\\movie",
    });
    const fallbackSessionId = "agent-session-new";
    expect(
      shouldInjectHistoryContext(
        { armed: true, injectedSessionId: hint?.sessionId },
        fallbackSessionId,
      ),
    ).toBe(true);
    expect(
      shouldInjectHistoryContext(
        { armed: true, injectedSessionId: fallbackSessionId },
        fallbackSessionId,
      ),
    ).toBe(false);
  });
});

describe("canAdoptResumeTarget", () => {
  it("adopts when the target is already the live session", () => {
    expect(
      canAdoptResumeTarget({
        targetIsLiveSession: true,
        sessionActive: true,
        transitionBlocked: true,
      }),
    ).toBe(true);
  });

  it("adopts when no session is active, letting auto-connect take over", () => {
    expect(
      canAdoptResumeTarget({
        targetIsLiveSession: false,
        sessionActive: false,
        transitionBlocked: true,
      }),
    ).toBe(true);
  });

  it("adopts when the current session can be closed right away", () => {
    expect(
      canAdoptResumeTarget({
        targetIsLiveSession: false,
        sessionActive: true,
        transitionBlocked: false,
      }),
    ).toBe(true);
  });

  it("refuses while a turn is running, so the summary fallback stays armed", () => {
    expect(
      canAdoptResumeTarget({
        targetIsLiveSession: false,
        sessionActive: true,
        transitionBlocked: true,
      }),
    ).toBe(false);
  });
});
