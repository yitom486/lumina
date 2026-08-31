import { beforeEach, describe, expect, it, vi } from "vitest";

import { useUiStore } from "./uiStore";

beforeEach(() => {
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    cb(0);
    return 0;
  });
  useUiStore.setState({
    fullscreen: false,
    sidebarTab: "transcript",
  });
});

describe("useUiStore sidebar", () => {
  it("switches sidebar tabs without chat state", () => {
    useUiStore.getState().setSidebarTab("notes");
    expect(useUiStore.getState().sidebarTab).toBe("notes");
    useUiStore.getState().setSidebarTab("transcript");
    expect(useUiStore.getState().sidebarTab).toBe("transcript");
  });
});
