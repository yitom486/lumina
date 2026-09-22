import { beforeEach, describe, expect, it, vi } from "vitest";

import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

import {
  clearPersistedSessionHintFor,
  fetchHintFromStore,
  flushChatRestore,
  persistSessionHintFor,
  resetChatStoreEphemeralState,
} from "./chatRestore";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const SESSION_KEY = "lumina-acp-session";

// B3 hint 主层 mock DB：SQLite 的单 profile hint 行。
type MockHintRow = { profileId: string; sessionId: string; cwd: string };
const mockHintTable = new Map<string, MockHintRow>();
let chatHintDown = false;

function hintInputOf(args: unknown): Record<string, unknown> {
  if (typeof args !== "object" || args === null) return {};
  const input = (args as { input?: unknown }).input;
  if (typeof input !== "object" || input === null) return {};
  return input as Record<string, unknown>;
}

beforeEach(() => {
  localStorage.clear();
  resetChatStoreEphemeralState();
  mockHintTable.clear();
  chatHintDown = false;
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (typeof cmd === "string" && cmd.startsWith("chat_hint_")) {
      if (chatHintDown) {
        return Promise.reject({
          code: "StorageError",
          message: "聊天记录暂时不可用，请重试",
        });
      }
      const input = hintInputOf(args);
      if (cmd === "chat_hint_upsert") {
        const row: MockHintRow = {
          profileId: String(input.profileId ?? ""),
          sessionId: String(input.sessionId ?? ""),
          cwd: String(input.cwd ?? ""),
        };
        mockHintTable.set(row.profileId, row);
        return Promise.resolve({ ...row, updatedAtMs: Date.now() });
      }
      if (cmd === "chat_hint_get") {
        const row = mockHintTable.get(String(input.profileId ?? "")) ?? null;
        return Promise.resolve(
          row ? { ...row, updatedAtMs: Date.now() } : null,
        );
      }
      if (cmd === "chat_hint_delete") {
        const existed = mockHintTable.delete(String(input.profileId ?? ""));
        return Promise.resolve(existed);
      }
    }
    return Promise.resolve(null);
  });
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

describe("session hint sqlite write-through (B3 SQLite only)", () => {
  const codexHint = {
    sessionId: "codex-thread",
    profileId: "codex",
    cwd: "D:\\movie",
  };
  const claudeHint = {
    sessionId: "claude-thread",
    profileId: "claude",
    cwd: "D:\\movie",
  };

  it("persists hints to SQLite while memory stays the source of truth", async () => {
    // 内存同步落键（热路径只读这里），DB 走 void 写穿。
    useAcpSessionStore.getState().setSavedSessionFor("codex", codexHint);
    useAcpSessionStore.getState().setSavedSessionFor("claude", claudeHint);
    persistSessionHintFor("codex", codexHint);
    persistSessionHintFor("claude", claudeHint);
    await flushChatRestore();

    // DB 主 + 内存镜像双断言：两家 hint 各归各的行。
    expect(mockHintTable.get("codex")).toMatchObject({
      sessionId: "codex-thread",
    });
    expect(mockHintTable.get("claude")).toMatchObject({
      sessionId: "claude-thread",
    });
    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-thread");
  });

  it("deletes the DB row on clear without touching other profiles", async () => {
    useAcpSessionStore.getState().setSavedSessionFor("codex", codexHint);
    useAcpSessionStore.getState().setSavedSessionFor("claude", claudeHint);
    persistSessionHintFor("codex", codexHint);
    persistSessionHintFor("claude", claudeHint);
    await flushChatRestore();
    expect(mockHintTable.get("codex")).toBeDefined();

    // 内存由调用方同步清，DB 由 helper void 删。
    useAcpSessionStore.getState().clearSavedSessionFor("codex");
    clearPersistedSessionHintFor("codex");
    await flushChatRestore();

    expect(mockHintTable.get("codex")).toBeUndefined();
    expect(mockHintTable.get("claude")?.sessionId).toBe("claude-thread");
    expect(useAcpSessionStore.getState().savedSessionFor("codex")).toBeNull();
  });

  it("keeps memory chat usable when SQLite is down and retries on next flush", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      chatHintDown = true;
      useAcpSessionStore.getState().setSavedSessionFor("codex", codexHint);
      persistSessionHintFor("codex", codexHint);
      await flushChatRestore();

      // DB 不可用：内存可聊（connect 照读这里），DB 还是空的。
      expect(
        useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
      ).toBe("codex-thread");
      expect(mockHintTable.get("codex")).toBeUndefined();

      chatHintDown = false;
      await flushChatRestore();
      expect(mockHintTable.get("codex")?.sessionId).toBe("codex-thread");
    } finally {
      errorSpy.mockRestore();
    }
  });

  it("hydrates hints from SQLite and rejects dirty rows", async () => {
    mockHintTable.set("codex", { ...codexHint });
    mockHintTable.set("broken", { profileId: "broken", sessionId: "", cwd: "" });

    expect(await fetchHintFromStore("codex")).toMatchObject({
      sessionId: "codex-thread",
      profileId: "codex",
    });
    expect(await fetchHintFromStore("broken")).toBeNull();
    expect(await fetchHintFromStore("cursor")).toBeNull();
  });
});
