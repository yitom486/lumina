import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";
import { useAcpProfilesStore } from "@lumina/chat-ui/acpProfilesStore";
import { useAcpSessionStore } from "@lumina/chat-ui/acpSessionStore";

import { AcpPanel } from "./AcpPanel";
import {
  CHAT_RESTORE_KEY,
  CHAT_STORE_MIGRATED_MARK,
  chatRestoreKeyFor,
  fetchHintFromStore,
  fetchSnapshotFromStore,
  flushChatRestore,
  migrateLegacyChatStoreOnce,
  readChatRestore,
  resetChatStoreEphemeralState,
  scheduleClearChatRestore,
  schedulePersistChatRestore,
} from "../chatRestore";
import { useChatSnapshotStore } from "../chatSnapshotStore";
import { useChatStoreHydrate } from "../useChatStoreHydrate";

type ChannelHandler = { onmessage: ((event: unknown) => void) | null };

const { channels } = vi.hoisted(() => ({
  channels: [] as ChannelHandler[],
}));

// B3 mock DB：内存表模拟 SQLite 主层，chatStoreDown 模拟 DB 不可用。
// 行结构与 Rust DTO（camelCase）对齐：快照按 (profile, session) 复合键，
type MockSnapshotRow = {
  profileId: string;
  sessionId: string;
  cwd: string | null;
  draft: string;
  turnsJson: string;
  updatedAtMs: number;
};
type MockHintRow = {
  profileId: string;
  sessionId: string;
  cwd: string;
  updatedAtMs: number;
};

const mockSnapshotTable = new Map<string, MockSnapshotRow>();
const mockHintTable = new Map<string, MockHintRow>();
let chatStoreDown = false;

function snapshotDbKey(profileId: string, sessionId: string): string {
  return `${profileId}\0${sessionId}`;
}

function chatStoreInputOf(args: unknown): Record<string, unknown> {
  if (typeof args !== "object" || args === null) return {};
  const input = (args as { input?: unknown }).input;
  if (typeof input !== "object" || input === null) return {};
  return input as Record<string, unknown>;
}

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((event: unknown) => void) | null = null;
    constructor() {
      channels.push(this);
    }
  },
}));

import { invoke } from "@tauri-apps/api/core";

const mockStatus = {
  available: true,
  adapterFound: true,
  codexFound: true,
  activeProfileId: "codex",
  profiles: [],
  message: "ok",
  sessionActive: false,
  busy: false,
};

function seedDbSnapshot(
  profileId: string,
  sessionId: string,
  userText: string,
  answer: string,
) {
  mockSnapshotTable.set(snapshotDbKey(profileId, sessionId), {
    profileId,
    sessionId,
    cwd: null,
    draft: "",
    turnsJson: JSON.stringify([
      {
        id: `db-${profileId}`,
        userText,
        answer,
        status: "done",
        activities: [],
        showActivities: false,
      },
    ]),
    updatedAtMs: 1,
  });
}

function renderPanel() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <AcpPanel />
    </QueryClientProvider>,
  );
}

async function waitForConnect() {
  await waitFor(() => {
    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_connect"),
    ).toHaveLength(1);
  });
  expect(channels[0]?.onmessage).toBeTypeOf("function");
}

function connectSessionIds(): (string | null)[] {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "acp_connect")
    .map(
      ([, args]) =>
        (args as { savedSession?: { sessionId?: string } | null })?.savedSession
          ?.sessionId ?? null,
    );
}

beforeEach(() => {
  channels.length = 0;
  localStorage.clear();
  // B3 SQLite only：渲染用例直接播 DB 种子（hydrate 异步回填），不再预置
  // 备层种子；迁移语义由本文件的 migrateOnce 专测覆盖。
  resetChatStoreEphemeralState();
  mockSnapshotTable.clear();
  mockHintTable.clear();
  chatStoreDown = false;
  useAcpSessionStore.setState({ savedSessions: {} });
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
  usePlayerStore.setState({ currentFile: null, status: "Idle" });
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (typeof cmd === "string" && cmd.startsWith("chat_")) {
      if (chatStoreDown) {
        return Promise.reject({
          code: "StorageError",
          message: "聊天记录暂时不可用，请重试",
        });
      }
      const input = chatStoreInputOf(args);
      switch (cmd) {
        case "chat_snapshot_upsert": {
          const row: MockSnapshotRow = {
            profileId: asString(input.profileId),
            sessionId: asString(input.sessionId),
            cwd:
              input.cwd === null || typeof input.cwd === "string"
                ? (input.cwd as string | null)
                : null,
            draft: asString(input.draft),
            turnsJson: asString(input.turnsJson, "[]"),
            updatedAtMs: Date.now(),
          };
          mockSnapshotTable.set(
            snapshotDbKey(row.profileId, row.sessionId),
            row,
          );
          return Promise.resolve(row);
        }
        case "chat_snapshot_get": {
          const row =
            mockSnapshotTable.get(
              snapshotDbKey(
                asString(input.profileId),
                asString(input.sessionId),
              ),
            ) ?? null;
          return Promise.resolve(row);
        }
        case "chat_snapshot_delete": {
          const existed = mockSnapshotTable.delete(
            snapshotDbKey(asString(input.profileId), asString(input.sessionId)),
          );
          return Promise.resolve(existed);
        }
        case "chat_hint_upsert": {
          const row: MockHintRow = {
            profileId: asString(input.profileId),
            sessionId: asString(input.sessionId),
            cwd: asString(input.cwd),
            updatedAtMs: Date.now(),
          };
          mockHintTable.set(row.profileId, row);
          return Promise.resolve(row);
        }
        case "chat_hint_get":
          return Promise.resolve(
            mockHintTable.get(asString(input.profileId)) ?? null,
          );
        case "chat_hint_delete": {
          const existed = mockHintTable.delete(asString(input.profileId));
          return Promise.resolve(existed);
        }
        default:
          return Promise.resolve(null);
      }
    }
    if (cmd === "acp_status") return Promise.resolve(mockStatus);
    if (cmd === "acp_connect") return Promise.resolve(null);
    if (cmd === "acp_load_session")
      return Promise.resolve([
        { role: "user", text: "cursor 远端老问题" },
        { role: "agent", text: "cursor 远端老回答" },
      ]);
    return Promise.resolve(null);
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("chat restore persistence (happy-dom localStorage)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("writes a throttled snapshot to the profile-scoped key and reads it back", () => {
    schedulePersistChatRestore({
      profileId: "codex",
      cwd: "D:\\movie",
      draft: "未发完",
      turns: [
        {
          id: "t1",
          userText: "问题",
          answer: "上次的回答",
          status: "done",
          activities: [],
          showActivities: false,
        },
      ],
    });
    // 节流：时间没到不写 DB，但内存镜像同步可见；备层从不写入。
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
    vi.advanceTimersByTime(1500);

    const snapshot = readChatRestore("codex");
    expect(snapshot?.profileId).toBe("codex");
    expect(snapshot?.cwd).toBe("D:\\movie");
    expect(snapshot?.draft).toBe("未发完");
    expect(snapshot?.turns.map((turn) => turn.id)).toEqual(["t1"]);
    // 别家的键纹丝不动。
    expect(localStorage.getItem(chatRestoreKeyFor("claude"))).toBeNull();
  });

  it("removes only the owning profile snapshot when there is nothing worth restoring", async () => {
    // B3：落定只写内存 + DB，从不写备层；clear 只删自家 DB 行 + 内存。
    schedulePersistChatRestore({
      profileId: "codex",
      sessionId: "s-codex",
      cwd: null,
      draft: "",
      turns: [
        {
          id: "t-codex",
          userText: "codex 问题",
          answer: "codex 回答",
          status: "done",
          activities: [],
          showActivities: false,
        },
      ],
    });
    schedulePersistChatRestore({
      profileId: "claude",
      sessionId: "s-claude",
      cwd: null,
      draft: "",
      turns: [
        {
          id: "t-claude",
          userText: "别家问题",
          answer: "别家回答",
          status: "done",
          activities: [],
          showActivities: false,
        },
      ],
    });
    await vi.advanceTimersByTimeAsync(1500);
    expect(
      mockSnapshotTable.get(snapshotDbKey("codex", "s-codex")),
    ).toBeDefined();

    scheduleClearChatRestore("codex", "s-codex");
    await vi.advanceTimersByTimeAsync(1500);
    expect(
      mockSnapshotTable.get(snapshotDbKey("codex", "s-codex")),
    ).toBeUndefined();
    expect(readChatRestore("codex", "s-codex")).toBeNull();
    // 别家不受影响，备层从头到尾没被写过。
    expect(readChatRestore("claude", "s-claude")?.turns.map((turn) => turn.id)).toEqual([
      "t-claude",
    ]);
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
    expect(localStorage.getItem(chatRestoreKeyFor("claude"))).toBeNull();
  });

  it("rejects dirty snapshots instead of rendering them", () => {
    // 迁移前备层一次性读取路径：任何非法直接 null（绝不渲染脏数据）。
    for (const dirty of [
      "not-json",
      JSON.stringify({ version: 999, profileId: "codex", turns: [] }),
      JSON.stringify({ version: 1, turns: [] }),
      JSON.stringify({ version: 1, profileId: "codex", cwd: 42, turns: [] }),
      JSON.stringify({ version: 1, profileId: "codex", draft: "", turns: [] }),
    ]) {
      localStorage.setItem(chatRestoreKeyFor("codex"), dirty);
      expect(readChatRestore("codex")).toBeNull();
    }
  });

  it("falls back to the legacy single key only on profile match", () => {
    localStorage.setItem(
      CHAT_RESTORE_KEY,
      JSON.stringify({
        version: 1,
        profileId: "codex",
        cwd: null,
        draft: "",
        turns: [
          {
            id: "legacy-1",
            userText: "升级前的旧快照",
            answer: "旧回答",
            status: "done",
            activities: [],
            showActivities: false,
          },
        ],
        updatedAtMs: 1,
      }),
    );
    // 同 profile 认一次（由一次性迁移收敛进 DB），别家的一律拒掉。
    expect(
      readChatRestore("codex")?.turns.map((turn) => turn.id),
    ).toEqual(["legacy-1"]);
    expect(readChatRestore("claude")).toBeNull();
  });

  it("refuses blank profile ids instead of writing to the bare key", () => {
    schedulePersistChatRestore({
      profileId: "  ",
      cwd: null,
      draft: "x",
      turns: [],
    });
    scheduleClearChatRestore("  ");
    vi.advanceTimersByTime(1500);
    expect(localStorage.getItem(CHAT_RESTORE_KEY)).toBeNull();
  });
});

describe("chat restore blackbox: instant render + auto resume", () => {
  it("renders the cached thread instantly with zero network before connect", async () => {
    seedDbSnapshot("codex", "", "上次退出前的问题", "上次退出前的回答");
    renderPanel();

    // SQLite 水合回填（仍零网络）：首帧空白随后 reseeding，不闪旧世界。
    // 首句会同时出现在工具栏标题与气泡里，所以用 All 断言。
    const cached = await screen.findAllByText("上次退出前的问题");
    expect(cached.length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("上次退出前的回答")).toBeInTheDocument();
  });

  it("passes the persisted session hint to acp_connect for resume", async () => {
    seedDbSnapshot("codex", "old-thread", "上次退出前的问题", "上次退出前的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "old-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    renderPanel();

    await waitFor(() => {
      const connect = vi
        .mocked(invoke)
        .mock.calls.find(([cmd]) => cmd === "acp_connect");
      expect(connect?.[1]).toMatchObject({
        savedSession: expect.objectContaining({ sessionId: "old-thread" }),
      });
    });
  });

  it("keeps cached turns when the backend resumes the same session", async () => {
    seedDbSnapshot("codex", "old-thread", "上次退出前的问题", "上次退出前的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "old-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    renderPanel();
    expect((await screen.findAllByText("上次退出前的问题")).length).toBeGreaterThanOrEqual(1);
    await waitForConnect();

    await act(async () => {
      channels[0]?.onmessage?.({
        type: "sessionSaved",
        sessionId: "old-thread",
        profileId: "codex",
        cwd: "D:\\movie",
        resume: "resumed",
      });
    });

    expect(
      (await screen.findAllByText("上次退出前的问题")).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("drops cached turns when the backend lands on a different new session", async () => {
    seedDbSnapshot("codex", "old-thread", "上次退出前的问题", "上次退出前的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "old-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    renderPanel();
    expect((await screen.findAllByText("上次退出前的问题")).length).toBeGreaterThanOrEqual(1);
    await waitForConnect();

    await act(async () => {
      channels[0]?.onmessage?.({
        type: "sessionSaved",
        sessionId: "brand-new",
        profileId: "codex",
        cwd: "D:\\movie",
        resume: null,
      });
    });

    await waitFor(() => {
      expect(screen.queryAllByText("上次退出前的问题")).toHaveLength(0);
    });
    expect(screen.queryByText("上次退出前的回答")).not.toBeInTheDocument();
  });

  it("ignores snapshots from another profile", async () => {
    seedDbSnapshot("claude", "claude-thread", "别的画像的问题", "别的画像的回答");
    renderPanel();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "新建对话" })).toBeInTheDocument();
    });
    expect(screen.queryByText("别的画像的问题")).not.toBeInTheDocument();
  });

  it("keeps each agent's session hint and snapshot across profile switches", async () => {
    seedDbSnapshot("codex", "codex-thread", "codex 的问题", "codex 的回答");
    seedDbSnapshot("claude", "claude-thread", "claude 的问题", "claude 的回答");
    useAcpSessionStore.getState().setSavedSessionFor("codex", {
      sessionId: "codex-thread",
      profileId: "codex",
      cwd: "D:\\movie",
    });
    useAcpSessionStore.getState().setSavedSessionFor("claude", {
      sessionId: "claude-thread",
      profileId: "claude",
      cwd: "D:\\movie",
    });
    renderPanel();

    // codex 世界：自家快照经 SQLite 水合摆出来。
    expect((await screen.findAllByText("codex 的问题")).length).toBeGreaterThanOrEqual(1);
    await waitForConnect();

    // 切到 claude：面板换成 claude 的快照，codex 的 hint 原样保留（以前这里会被删掉）。
    await act(async () => {
      useAcpProfilesStore.setState({ activeProfileId: "claude" });
    });
    expect((await screen.findAllByText("claude 的问题")).length).toBeGreaterThanOrEqual(1);
    expect(screen.queryByText("codex 的问题")).not.toBeInTheDocument();
    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-thread");
    expect(
      useAcpSessionStore.getState().savedSessionFor("claude")?.sessionId,
    ).toBe("claude-thread");

    // 切回 codex：自家快照与 hint 都在，直接 resume。
    await act(async () => {
      useAcpProfilesStore.setState({ activeProfileId: "codex" });
    });
    expect((await screen.findAllByText("codex 的问题")).length).toBeGreaterThanOrEqual(1);
    expect(
      useAcpSessionStore.getState().savedSessionFor("codex")?.sessionId,
    ).toBe("codex-thread");

    // 三次 connect 带的都是当时世界的自家 hint，绝不串台。
    await waitFor(() => {
      expect(
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_connect"),
      ).toHaveLength(3);
    });
    expect(connectSessionIds()).toEqual([
      "codex-thread",
      "claude-thread",
      "codex-thread",
    ]);
  });

  it("reloads the resumed thread text when the panel has no cached snapshot", async () => {
    // 快照缺失（localStorage 是空的）但 hint 还在：resume 续记忆，
    // 这次 reload 把远端真实文本摆出来——切到有记忆的 Agent 不再看空白框。
    useAcpSessionStore.getState().setSavedSessionFor("cursor", {
      sessionId: "cursor-thread",
      profileId: "cursor",
      cwd: "D:\\movie",
    });
    useAcpProfilesStore.setState({ activeProfileId: "cursor" });
    renderPanel();
    await waitForConnect();

    await act(async () => {
      channels[0]?.onmessage?.({
        type: "sessionSaved",
        sessionId: "cursor-thread",
        profileId: "cursor",
        cwd: "D:\\movie",
        resume: "resumed",
      });
    });

    expect(
      (await screen.findAllByText("cursor 远端老问题")).length,
    ).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("cursor 远端老回答")).toBeInTheDocument();
    // 文本走的是自家线程的 load，不是编出来的。
    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "acp_load_session"),
    ).not.toHaveLength(0);
  });
});

function b2Turn(id: string, userText: string) {
  return {
    id,
    userText,
    answer: "答",
    status: "done" as const,
    activities: [],
    showActivities: false,
  };
}

function b2SeedLocal(profileId: string, draft: string, userText: string) {
  localStorage.setItem(
    chatRestoreKeyFor(profileId),
    JSON.stringify({
      version: 1,
      profileId,
      cwd: null,
      draft,
      turns: [b2Turn(`local-${profileId}`, userText)],
      updatedAtMs: 1,
    }),
  );
}

describe("sqlite only write-through (B3)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("writes the trailing snapshot to SQLite and memory without touching localStorage", async () => {
    schedulePersistChatRestore({
      profileId: "codex",
      sessionId: "s-codex",
      cwd: "D:\\movie",
      draft: "主层草稿",
      turns: [b2Turn("t1", "主层问题")],
    });
    await vi.advanceTimersByTimeAsync(1500);

    const dbRow = mockSnapshotTable.get(snapshotDbKey("codex", "s-codex"));
    expect(dbRow?.draft).toBe("主层草稿");
    expect(
      JSON.parse(dbRow?.turnsJson ?? "[]").map(
        (turn: { id: string }) => turn.id,
      ),
    ).toEqual(["t1"]);
    // DB 主 + 内存镜像双断言；备层从头到尾没被写过。
    const mirrored = readChatRestore("codex", "s-codex");
    expect(mirrored?.draft).toBe("主层草稿");
    expect(mirrored?.turns.map((turn) => turn.id)).toEqual(["t1"]);
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
  });

  it("deletes from SQLite and memory when there is nothing worth restoring", async () => {
    schedulePersistChatRestore({
      profileId: "codex",
      sessionId: "s-codex",
      cwd: null,
      draft: "待删",
      turns: [b2Turn("t1", "待删问题")],
    });
    await vi.advanceTimersByTimeAsync(1500);
    expect(
      mockSnapshotTable.get(snapshotDbKey("codex", "s-codex")),
    ).toBeDefined();

    scheduleClearChatRestore("codex", "s-codex");
    await vi.advanceTimersByTimeAsync(1500);

    expect(
      mockSnapshotTable.get(snapshotDbKey("codex", "s-codex")),
    ).toBeUndefined();
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
    expect(readChatRestore("codex", "s-codex")).toBeNull();
  });

  it("keeps fast-switch worlds apart: stale trailing still lands on its own keys", async () => {
    // 快切 A→B→A：旧 trailing 照样落旧键，新调度覆盖同槽，绝不串台。
    schedulePersistChatRestore({
      profileId: "codex",
      sessionId: "s-codex",
      cwd: null,
      draft: "codex 草稿 v1",
      turns: [b2Turn("t-c1", "codex 问 v1")],
    });
    schedulePersistChatRestore({
      profileId: "claude",
      sessionId: "s-claude",
      cwd: null,
      draft: "claude 草稿",
      turns: [b2Turn("t-l1", "claude 问")],
    });
    schedulePersistChatRestore({
      profileId: "codex",
      sessionId: "s-codex",
      cwd: null,
      draft: "codex 草稿 v2",
      turns: [b2Turn("t-c2", "codex 问 v2")],
    });
    await vi.advanceTimersByTimeAsync(1500);

    expect(
      mockSnapshotTable.get(snapshotDbKey("codex", "s-codex"))?.draft,
    ).toBe("codex 草稿 v2");
    expect(
      mockSnapshotTable.get(snapshotDbKey("claude", "s-claude"))?.draft,
    ).toBe("claude 草稿");
    expect(readChatRestore("codex", "s-codex")?.draft).toBe("codex 草稿 v2");
    expect(readChatRestore("claude", "s-claude")?.draft).toBe("claude 草稿");
  });

  it("flushes pending writes on unmount without loss", async () => {
    schedulePersistChatRestore({
      profileId: "codex",
      sessionId: "s-codex",
      cwd: null,
      draft: "卸载前草稿",
      turns: [b2Turn("t1", "卸载前问题")],
    });
    // timer 还没到期就卸载：显式 flush 一次写完。
    await flushChatRestore();

    expect(
      mockSnapshotTable.get(snapshotDbKey("codex", "s-codex"))?.draft,
    ).toBe("卸载前草稿");
    expect(readChatRestore("codex", "s-codex")?.draft).toBe("卸载前草稿");
  });

  it("keeps chatting from memory when SQLite is down and retries later", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      chatStoreDown = true;
      schedulePersistChatRestore({
        profileId: "codex",
        sessionId: "s-codex",
        cwd: null,
        draft: "离线草稿",
        turns: [b2Turn("t1", "离线问题")],
      });
      await vi.advanceTimersByTimeAsync(1500);

      // DB 不可用：内存可聊（备层从不写入），DB 还是空的。
      expect(readChatRestore("codex", "s-codex")?.draft).toBe("离线草稿");
      expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
      expect(mockSnapshotTable.size).toBe(0);

      // 恢复后下次 flush 重试（回队条目），不再丢。
      chatStoreDown = false;
      await flushChatRestore();
      expect(
        mockSnapshotTable.get(snapshotDbKey("codex", "s-codex"))?.draft,
      ).toBe("离线草稿");
    } finally {
      errorSpy.mockRestore();
    }
  });
});

describe("migrateLegacyChatStoreOnce", () => {
  function seedValidTurn(id: string, userText: string) {
    return {
      id,
      userText,
      answer: "答",
      status: "done",
      activities: [],
      showActivities: false,
    };
  }

  beforeEach(() => {
    localStorage.removeItem(CHAT_STORE_MIGRATED_MARK);
  });

  it("moves scoped and legacy snapshots plus hints into SQLite, then marks", async () => {
    localStorage.setItem(
      chatRestoreKeyFor("codex"),
      JSON.stringify({
        version: 1,
        profileId: "codex",
        cwd: "D:\\movie",
        draft: "待迁草稿",
        turns: [seedValidTurn("t1", "待迁问题")],
        updatedAtMs: 1,
      }),
    );
    localStorage.setItem(
      CHAT_RESTORE_KEY,
      JSON.stringify({
        version: 1,
        profileId: "claude",
        cwd: null,
        draft: "",
        turns: [seedValidTurn("legacy-1", "旧单键问题")],
        updatedAtMs: 1,
      }),
    );
    localStorage.setItem(
      "lumina-acp-session",
      JSON.stringify({
        state: {
          savedSessions: {
            codex: {
              sessionId: "s-codex",
              profileId: "codex",
              cwd: "D:\\movie",
            },
          },
        },
        version: 1,
      }),
    );

    expect(await migrateLegacyChatStoreOnce()).toBe("migrated");

    // 快照按 hint 会话归属进 DB（无 hint 的进空会话行）。
    expect(
      mockSnapshotTable.get(snapshotDbKey("codex", "s-codex"))?.draft,
    ).toBe("待迁草稿");
    expect(mockSnapshotTable.get(snapshotDbKey("claude", ""))).toBeDefined();
    expect(mockHintTable.get("codex")?.sessionId).toBe("s-codex");
    // 已迁恢复键删除 + 标记；会话 persist 本体保留作离线备。
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBeNull();
    expect(localStorage.getItem(CHAT_RESTORE_KEY)).toBeNull();
    expect(localStorage.getItem(CHAT_STORE_MIGRATED_MARK)).toBe("1");
    expect(localStorage.getItem("lumina-acp-session")).not.toBeNull();
  });

  it("skips dirty snapshots but still marks", async () => {
    localStorage.setItem(chatRestoreKeyFor("codex"), "not-json");

    expect(await migrateLegacyChatStoreOnce()).toBe("migrated");

    expect(mockSnapshotTable.size).toBe(0);
    // 脏数据跳过：不迁、不删，读路径自然拒掉。
    expect(localStorage.getItem(chatRestoreKeyFor("codex"))).toBe("not-json");
    expect(localStorage.getItem(CHAT_STORE_MIGRATED_MARK)).toBe("1");
  });

  it("defers without deleting when SQLite is down", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      b2SeedLocal("codex", "待迁草稿", "待迁问题");
      chatStoreDown = true;

      expect(await migrateLegacyChatStoreOnce()).toBe("deferred");

      expect(localStorage.getItem(chatRestoreKeyFor("codex"))).not.toBeNull();
      expect(localStorage.getItem(CHAT_STORE_MIGRATED_MARK)).toBeNull();
    } finally {
      errorSpy.mockRestore();
    }
  });

  it("short-circuits when already migrated", async () => {
    localStorage.setItem(CHAT_STORE_MIGRATED_MARK, "1");
    const callsBefore = vi.mocked(invoke).mock.calls.length;

    expect(await migrateLegacyChatStoreOnce()).toBe("already");

    expect(
      vi
        .mocked(invoke)
        .mock.calls.slice(callsBefore)
        .filter(([cmd]) => String(cmd).startsWith("chat_")),
    ).toHaveLength(0);
  });
});

describe("fetchSnapshotFromStore / fetchHintFromStore validation", () => {
  it("rejects dirty snapshot rows instead of rendering them", async () => {
    mockSnapshotTable.set(snapshotDbKey("codex", "s-bad-json"), {
      profileId: "codex",
      sessionId: "s-bad-json",
      cwd: null,
      draft: "x",
      turnsJson: "not-json",
      updatedAtMs: 1,
    });
    mockSnapshotTable.set(snapshotDbKey("codex", "s-not-array"), {
      profileId: "codex",
      sessionId: "s-not-array",
      cwd: null,
      draft: "x",
      turnsJson: JSON.stringify({ nope: true }),
      updatedAtMs: 1,
    });
    mockSnapshotTable.set(snapshotDbKey("codex", "s-empty"), {
      profileId: "codex",
      sessionId: "s-empty",
      cwd: null,
      draft: "  ",
      turnsJson: "[]",
      updatedAtMs: 1,
    });

    expect(await fetchSnapshotFromStore("codex", "s-bad-json")).toBeNull();
    expect(await fetchSnapshotFromStore("codex", "s-not-array")).toBeNull();
    expect(await fetchSnapshotFromStore("codex", "s-empty")).toBeNull();
    expect(await fetchSnapshotFromStore("codex", "s-missing")).toBeNull();
  });

  it("reads a valid snapshot row with frozen streaming turns", async () => {
    mockSnapshotTable.set(snapshotDbKey("codex", "s1"), {
      profileId: "codex",
      sessionId: "s1",
      cwd: "D:\\movie",
      draft: "主层草稿",
      turnsJson: JSON.stringify([
        {
          id: "t1",
          userText: "主层问题",
          answer: "半截",
          status: "streaming",
          activities: [],
          showActivities: false,
        },
      ]),
      updatedAtMs: 7,
    });

    const snapshot = await fetchSnapshotFromStore("codex", "s1");
    expect(snapshot?.draft).toBe("主层草稿");
    expect(snapshot?.turns.map((turn) => turn.status)).toEqual(["done"]);
  });

  it("rejects dirty hint rows", async () => {
    mockHintTable.set("codex", {
      profileId: "codex",
      sessionId: "",
      cwd: "",
      updatedAtMs: 1,
    });

    expect(await fetchHintFromStore("codex")).toBeNull();
    expect(await fetchHintFromStore("cursor")).toBeNull();
  });
});

describe("useChatStoreHydrate epoch guard", () => {
  it("discards stale hydrate responses after a fast profile switch", async () => {
    let resolveCodexGet: ((value: unknown) => void) | null = null;
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "chat_snapshot_get") {
        const input = chatStoreInputOf(args);
        if (input.profileId === "codex") {
          return new Promise((resolve) => {
            resolveCodexGet = resolve;
          });
        }
        return Promise.resolve({
          profileId: "claude",
          sessionId: "s-claude",
          cwd: null,
          draft: "claude 新世界",
          turnsJson: JSON.stringify([
            {
              id: "t-l",
              userText: "claude 问",
              answer: "答",
              status: "done",
              activities: [],
              showActivities: false,
            },
          ]),
          updatedAtMs: 5,
        });
      }
      if (cmd === "chat_hint_get") return Promise.resolve(null);
      if (cmd === "acp_status") return Promise.resolve(mockStatus);
      return Promise.resolve(null);
    });
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const { rerender, unmount } = renderHook(
      ({
        profileId,
        sessionId,
      }: {
        profileId: string;
        sessionId: string | null;
      }) =>
        useChatStoreHydrate({ queryClient: client, profileId, sessionId }),
      { initialProps: { profileId: "codex", sessionId: "s-codex" } },
    );

    // 快切到 claude 后，codex 的迟到回包才到达：必须被 seq 守卫丢弃。
    rerender({ profileId: "claude", sessionId: "s-claude" });
    await act(async () => {
      resolveCodexGet?.({
        profileId: "codex",
        sessionId: "s-codex",
        cwd: null,
        draft: "codex 旧世界",
        turnsJson: JSON.stringify([
          {
            id: "t-c",
            userText: "codex 问",
            answer: "答",
            status: "done",
            activities: [],
            showActivities: false,
          },
        ]),
        updatedAtMs: 1,
      });
    });

    await waitFor(() => {
      expect(
        useChatSnapshotStore.getState().snapshotFor("claude", "s-claude")
          ?.draft,
      ).toBe("claude 新世界");
    });
    expect(
      useChatSnapshotStore.getState().snapshotFor("codex", "s-codex"),
    ).toBeNull();
    unmount();
  });

  it("migrates the pre-migration localStorage backup into SQLite once", async () => {
    // 迁移前一次性读取：备层种子经 migrateOnce 收敛进 DB 后摆出来，
    // 标记置位后备层不再被读写。
    b2SeedLocal("codex", "备层草稿", "备层问题");
    renderPanel();

    expect(
      (await screen.findAllByText("备层问题")).length,
    ).toBeGreaterThanOrEqual(1);
    await waitFor(() => {
      expect(mockSnapshotTable.get(snapshotDbKey("codex", ""))?.draft).toBe(
        "备层草稿",
      );
    });
  });
});
