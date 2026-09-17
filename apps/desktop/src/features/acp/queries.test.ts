import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { acpQueryKeys, prefetchAgentSessionList } from "./queries";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {},
}));

import { invoke } from "@tauri-apps/api/core";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("acpQueryKeys", () => {
  it("sessionList key is stable per scope", () => {
    expect(acpQueryKeys.sessionList("codex", "D:\\movie")).toEqual(
      acpQueryKeys.sessionList("codex", "D:\\movie"),
    );
  });

  it("sessionList key isolates scopes", () => {
    expect(acpQueryKeys.sessionList("codex", "D:\\a")).not.toEqual(
      acpQueryKeys.sessionList("codex", "D:\\b"),
    );
    expect(acpQueryKeys.sessionList("codex", "D:\\a")).not.toEqual(
      acpQueryKeys.sessionList("claude", "D:\\a"),
    );
    expect(acpQueryKeys.sessionList("codex", null)).toEqual(
      acpQueryKeys.sessionList("codex", null),
    );
  });

  it("transcript key isolates threads", () => {
    expect(
      acpQueryKeys.transcript("codex", "D:\\movie", "sess-1"),
    ).toEqual(acpQueryKeys.transcript("codex", "D:\\movie", "sess-1"));
    expect(
      acpQueryKeys.transcript("codex", "D:\\movie", "sess-1"),
    ).not.toEqual(acpQueryKeys.transcript("codex", "D:\\movie", "sess-2"));
    expect(
      acpQueryKeys.transcript("codex", "D:\\movie", "sess-1"),
    ).not.toEqual(acpQueryKeys.transcript("codex", "D:\\other", "sess-1"));
  });

  it("prefetch fills the list cache so reopening within staleTime skips IPC", async () => {
    vi.mocked(invoke).mockResolvedValue({
      verified: true,
      sessions: [],
      truncated: false,
    });
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    await prefetchAgentSessionList(client, {
      profileId: "codex",
      cwd: "D:\\movie",
    });
    await prefetchAgentSessionList(client, {
      profileId: "codex",
      cwd: "D:\\movie",
    });
    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith(
      "acp_list_agent_sessions",
      expect.objectContaining({ cwd: "D:\\movie" }),
    );
  });
});
