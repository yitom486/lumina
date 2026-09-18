import { describe, expect, it } from "vitest";

import {
  agentSessionListTrust,
  canSwitchHistoryConversation,
  historyThreadRows,
  resumeOutcomeNotice,
} from "./conversationContext";

describe("canSwitchHistoryConversation", () => {
  it("blocks while answering or creating a session", () => {
    expect(
      canSwitchHistoryConversation({ busy: true, creatingSession: false }),
    ).toBe(false);
    expect(
      canSwitchHistoryConversation({ busy: false, creatingSession: true }),
    ).toBe(false);
    expect(
      canSwitchHistoryConversation({ busy: false, creatingSession: false }),
    ).toBe(true);
  });
});

describe("historyThreadRows", () => {
  it("maps native sessions to rows, newest first", () => {
    const rows = historyThreadRows(
      [
        {
          sessionId: "old",
          cwd: "D:/movie",
          title: "旧",
          updatedAt: "2026-09-15T10:00:00Z",
          kind: null,
        },
        {
          sessionId: "new",
          cwd: "D:/movie",
          title: "新",
          updatedAt: "2026-09-16T10:00:00Z",
          kind: null,
        },
      ],
      {},
    );
    expect(rows.map((row) => row.sessionId)).toEqual(["new", "old"]);
    expect(rows[0]?.title).toBe("新");
  });

  it("prefers the in-memory title override, then native title, then short id", () => {
    const rows = historyThreadRows(
      [
        {
          sessionId: "sess-12345678",
          cwd: "D:/movie",
          title: "  ",
          updatedAt: null,
          kind: null,
        },
      ],
      { "sess-12345678": "用户首句" },
    );
    expect(rows[0]?.title).toBe("用户首句");
    const fallback = historyThreadRows(
      [
        {
          sessionId: "sess-12345678",
          cwd: "D:/movie",
          title: null,
          updatedAt: null,
          kind: null,
        },
      ],
      {},
    );
    expect(fallback[0]?.title).toBe("对话 sess-123");
  });

  it("drops blank ids and non-arrays", () => {
    expect(
      historyThreadRows(
        [
          {
            sessionId: "   ",
            cwd: "D:/movie",
            title: "x",
            updatedAt: null,
            kind: null,
          },
        ],
        {},
      ),
    ).toEqual([]);
    expect(historyThreadRows(null, {})).toEqual([]);
    expect(historyThreadRows(undefined, {})).toEqual([]);
  });

  it("falls back past scaffold echoes instead of displaying them", () => {
    const rows = historyThreadRows(
      [
        {
          sessionId: "sess-header",
          cwd: "D:/movie",
          title: "【工具优先】本轮优先使用 Lumina 本地工具……",
          updatedAt: "2026-09-17T16:20:00Z",
          kind: null,
        },
        {
          sessionId: "sess-file",
          cwd: "D:/movie",
          title: "[@Our.Beloved.Summer.2021.EP02.HD1080P.X264.AAC.Korean.CHS.Mp4er.mp4]",
          updatedAt: "2026-09-17T14:09:00Z",
          kind: null,
        },
        {
          sessionId: "sess-external",
          cwd: "D:/movie",
          title: null,
          updatedAt: "2026-09-17T13:00:00Z",
          kind: null,
        },
        {
          sessionId: "sess-human",
          cwd: "D:/movie",
          title: "这集讲了什么",
          updatedAt: "2026-09-17T12:00:00Z",
          kind: null,
        },
      ],
      {},
    );
    expect(rows.map((row) => row.title)).toEqual([
      "Lumina 对话",
      "Our Beloved Summer 2021 EP02 HD1080P X264",
      "对话 sess-ext",
      "这集讲了什么",
    ]);
  });

  it("marks only untitled own threads as empty candidates", () => {
    const byId = (id: string) =>
      historyThreadRows(
        [
          { sessionId: "mine-empty", cwd: "D:/m", title: null, updatedAt: null, kind: "chat" },
          { sessionId: "mine-named", cwd: "D:/m", title: "【工具优先】xxx", updatedAt: null, kind: "chat" },
          { sessionId: "alien-empty", cwd: "D:/m", title: null, updatedAt: null, kind: null },
          { sessionId: "alien-blank", cwd: "D:/m", title: "   ", updatedAt: null, kind: null },
        ],
        {},
      ).find((row) => row.sessionId === id);
    // 自家无标题：空占位候选，可批量清理。
    expect(byId("mine-empty")?.isEmptyCandidate).toBe(true);
    // 有原生标题（哪怕是脚手架）：可能有过消息，不标。
    expect(byId("mine-named")?.isEmptyCandidate).toBe(false);
    // 外部无标题：无法判断，不碰。
    expect(byId("alien-empty")?.isEmptyCandidate).toBe(false);
    expect(byId("alien-blank")?.isEmptyCandidate).toBe(false);
  });

  it("never marks threads with seen content as empty", () => {
    const [row] = historyThreadRows(
      [
        { sessionId: "s1", cwd: "D:/m", title: null, updatedAt: null, kind: "chat" },
      ],
      { s1: "用户首句" },
    );
    expect(row?.title).toBe("用户首句");
    expect(row?.isEmptyCandidate).toBe(false);
  });
});

describe("agentSessionListTrust", () => {
  it("matches on verified data, regardless of busy", () => {
    // 切换/回答期间缓存行依然有效：忙时禁点选与新拉取，不禁展示。
    expect(
      agentSessionListTrust({
        hasData: true,
        verified: true,
        truncated: false,
      }),
    ).toEqual({ canMatch: true, canAssertMissing: true });
    expect(
      agentSessionListTrust({
        hasData: true,
        verified: false,
        truncated: false,
      }).canMatch,
    ).toBe(false);
    expect(
      agentSessionListTrust({
        hasData: false,
        verified: true,
        truncated: false,
      }).canMatch,
    ).toBe(false);
  });
});

describe("resumeOutcomeNotice", () => {
  it("keeps occupied and unavailable apart", () => {
    expect(
      resumeOutcomeNotice({ outcome: "occupied", sessionMatchedRequest: true }),
    ).toContain("正被其它程序使用");
    expect(
      resumeOutcomeNotice({
        outcome: "unavailable",
        sessionMatchedRequest: true,
      }),
    ).toContain("已不存在");
    expect(
      resumeOutcomeNotice({ outcome: "resumed", sessionMatchedRequest: true }),
    ).toBe("已恢复该对话的 AI 记忆");
  });
});
