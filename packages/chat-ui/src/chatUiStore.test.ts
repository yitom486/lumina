import { beforeEach, describe, expect, it } from "vitest";

import {
  DEFAULT_DOCK_WIDTH,
  MAX_DOCK_WIDTH,
  MIN_DOCK_WIDTH,
  useChatUiStore,
} from "./chatUiStore";

beforeEach(() => {
  useChatUiStore.setState({
    showActivityWhileDone: false,
    chatMounted: false,
    chatOpen: false,
    acpResponding: false,
    dockWidth: DEFAULT_DOCK_WIDTH,
  });
});

describe("useChatUiStore chat lifecycle", () => {
  it("does not mount until user opens chat", () => {
    expect(useChatUiStore.getState().chatMounted).toBe(false);
  });

  it("mounts once and stays mounted when hidden", () => {
    useChatUiStore.getState().openChat();
    expect(useChatUiStore.getState().chatMounted).toBe(true);
    expect(useChatUiStore.getState().chatOpen).toBe(true);

    useChatUiStore.getState().closeChat();
    expect(useChatUiStore.getState().chatMounted).toBe(true);
    expect(useChatUiStore.getState().chatOpen).toBe(false);
  });

  it("toggle opens on first use then hides without unmounting", () => {
    useChatUiStore.getState().toggleChat();
    expect(useChatUiStore.getState().chatMounted).toBe(true);
    useChatUiStore.getState().toggleChat();
    expect(useChatUiStore.getState().chatOpen).toBe(false);
    expect(useChatUiStore.getState().chatMounted).toBe(true);
  });

  it("clamps the shared right panel width to usable bounds", () => {
    expect(useChatUiStore.getState().dockWidth).toBe(DEFAULT_DOCK_WIDTH);
    useChatUiStore.getState().setDockWidth(480);
    expect(useChatUiStore.getState().dockWidth).toBe(480);
    useChatUiStore.getState().setDockWidth(10_000);
    expect(useChatUiStore.getState().dockWidth).toBe(MAX_DOCK_WIDTH);
    useChatUiStore.getState().setDockWidth(-5);
    expect(useChatUiStore.getState().dockWidth).toBe(MIN_DOCK_WIDTH);
    useChatUiStore.getState().setDockWidth(Number.NaN);
    expect(useChatUiStore.getState().dockWidth).toBe(DEFAULT_DOCK_WIDTH);
  });
});
