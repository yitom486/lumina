import { beforeEach, describe, expect, it } from "vitest";

import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

const SESSION_KEY = "lumina-acp-session";

beforeEach(() => {
  localStorage.clear();
  useAcpSessionStore.setState({ savedSessions: {} });
});

describe("acp session store isolation", () => {
  it("keeps each agent's resume hint under its own profile key", () => {
    const store = useAcpSessionStore.getState();
    store.setSavedSessionFor("codex", {
      sessionId: "codex-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    store.setSavedSessionFor("claude", {
      sessionId: "claude-thread",
      profileId: "claude",
      cwd: "D:\\movie",
    });

    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-thread");
    expect(
      useAcpSessionStore.getState().savedSessionFor("claude")?.sessionId,
    ).toBe("claude-thread");
    // 没开过的 agent（比如将来的 cursor）就是没有，不会读到别家的。
    expect(useAcpSessionStore.getState().savedSessionFor("cursor")).toBeNull();
  });

  it("forces the key to win over a mismatched profileId field", () => {
    useAcpSessionStore.getState().setSavedSessionFor("claude", {
      sessionId: "x-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });

    expect(
      useAcpSessionStore.getState().savedSessionFor("claude"),
    ).toMatchObject({ sessionId: "x-thread", profileId: "claude" });
    expect(useAcpSessionStore.getState().savedSessionFor("codex")).toBeNull();
  });

  it("clearing one profile never touches the others", () => {
    const store = useAcpSessionStore.getState();
    store.setSavedSessionFor("codex", {
      sessionId: "codex-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    store.setSavedSessionFor("claude", {
      sessionId: "claude-thread",
      profileId: "claude",
      cwd: "D:\\movie",
    });

    useAcpSessionStore.getState().clearSavedSessionFor("codex");

    expect(useAcpSessionStore.getState().savedSessionFor("codex")).toBeNull();
    expect(
      useAcpSessionStore.getState().savedSessionFor("claude")?.sessionId,
    ).toBe("claude-thread");
  });

  it("ignores blank profile ids", () => {
    useAcpSessionStore.getState().setSavedSessionFor("  ", {
      sessionId: "x",
      profileId: "  ",
      cwd: "",
    });
    expect(useAcpSessionStore.getState().savedSessionFor("  ")).toBeNull();
    expect(useAcpSessionStore.getState().savedSessions).toEqual({});
  });

  it("persists the per-profile map so restart can resume each agent", () => {
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });

    const raw = localStorage.getItem(SESSION_KEY);
    expect(raw).not.toBeNull();
    const persisted = JSON.parse(raw ?? "{}") as {
      state?: { savedSessions?: Record<string, { sessionId?: string }> };
    };
    expect(
      persisted.state?.savedSessions?.["codex"]?.sessionId,
    ).toBe("codex-thread");
  });

  it("migrates the legacy single slot into the owning profile key", async () => {
    localStorage.setItem(
      SESSION_KEY,
      JSON.stringify({
        state: {
          savedSession: {
            sessionId: "legacy-thread",
            profileId: "codex",
            cwd: "D:\\movie",
          },
        },
        version: 0,
      }),
    );

    await useAcpSessionStore.persist.rehydrate();

    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("legacy-thread");
    expect(useAcpSessionStore.getState().savedSessionFor("claude")).toBeNull();
  });

  it("drops a legacy slot without a usable profile instead of guessing", async () => {
    localStorage.setItem(
      SESSION_KEY,
      JSON.stringify({
        state: { savedSession: { sessionId: "", profileId: "", cwd: "" } },
        version: 0,
      }),
    );

    await useAcpSessionStore.persist.rehydrate();

    expect(useAcpSessionStore.getState().savedSessions).toEqual({});
  });
});
