import { beforeEach, describe, expect, it } from "vitest";

import { useChatUiStore } from "./chatUiStore";

beforeEach(() => {
  useChatUiStore.setState({
    showActivityWhileDone: false,
    chatMounted: false,
    chatOpen: false,
    acpResponding: false,
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
});
