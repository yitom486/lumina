import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";

import { acpWatchFeedQueryKey, getAcpWatchFeed } from "./api";

vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {},
  invoke: vi.fn(),
}));

afterEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("AI watch-feed API", () => {
  it("uses a stable read-only query key and Tauri command", async () => {
    vi.mocked(invoke).mockResolvedValue({
      source: "empty",
      items: [],
    });

    await expect(getAcpWatchFeed()).resolves.toEqual({
      source: "empty",
      items: [],
    });
    expect(acpWatchFeedQueryKey).toEqual(["acp-watch-feed"]);
    expect(invoke).toHaveBeenCalledWith("acp_watch_feed");
  });
});
