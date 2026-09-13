import { beforeEach, describe, expect, it, vi } from "vitest";

const isFullscreen = vi.fn();
const setFullscreen = vi.fn();

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isFullscreen,
    setFullscreen,
  }),
}));

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
  isFullscreen.mockReset();
  setFullscreen.mockReset();
});

describe("useUiStore sidebar", () => {
  it("switches sidebar tabs without chat state", () => {
    useUiStore.getState().setSidebarTab("notes");
    expect(useUiStore.getState().sidebarTab).toBe("notes");
    useUiStore.getState().setSidebarTab("online");
    expect(useUiStore.getState().sidebarTab).toBe("online");
    useUiStore.getState().setSidebarTab("transcript");
    expect(useUiStore.getState().sidebarTab).toBe("transcript");
  });
});

describe("useUiStore fullscreen", () => {
  it("uses the native window state as the source of truth", async () => {
    isFullscreen.mockResolvedValue(true);

    await useUiStore.getState().syncFullscreen();

    expect(isFullscreen).toHaveBeenCalledOnce();
    expect(useUiStore.getState().fullscreen).toBe(true);
  });

  it("confirms the actual native state after requesting a change", async () => {
    setFullscreen.mockResolvedValue(undefined);
    isFullscreen.mockResolvedValue(true);

    await useUiStore.getState().setFullscreen(true);

    expect(setFullscreen).toHaveBeenCalledWith(true);
    expect(isFullscreen).toHaveBeenCalledOnce();
    expect(useUiStore.getState().fullscreen).toBe(true);
  });
});
