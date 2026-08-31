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
    acpPanelAlive: false,
  });
});

describe("useUiStore acpPanelAlive", () => {
  it("starts false until user opens the chat tab", () => {
    expect(useUiStore.getState().acpPanelAlive).toBe(false);
    useUiStore.getState().setSidebarTab("notes");
    expect(useUiStore.getState().acpPanelAlive).toBe(false);
  });

  it("stays true after leaving the chat tab", () => {
    useUiStore.getState().setSidebarTab("acp");
    expect(useUiStore.getState().acpPanelAlive).toBe(true);
    useUiStore.getState().setSidebarTab("transcript");
    expect(useUiStore.getState().acpPanelAlive).toBe(true);
    expect(useUiStore.getState().sidebarTab).toBe("transcript");
  });
});
