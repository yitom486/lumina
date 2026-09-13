import { beforeEach, describe, expect, it } from "vitest";

import { useNoteComposeStore } from "./noteComposeStore";

describe("useNoteComposeStore", () => {
  beforeEach(() => {
    useNoteComposeStore.setState({
      mediaPath: null,
      body: "草稿",
      includeQuotes: true,
      quoteMode: "manual",
      selectedIndices: [1, 2, 3],
      anchorCueIndex: 2,
      lastPickedCueIndex: 3,
      lastPickedListIndex: 5,
    });
  });

  it("resetCompose clears the whole draft", () => {
    useNoteComposeStore.getState().resetCompose();
    const state = useNoteComposeStore.getState();
    expect(state.body).toBe("");
    expect(state.quoteMode).toBe("auto");
    expect(state.selectedIndices).toEqual([]);
  });

  it("resetComposeKeepQuotes keeps quote picks for another saved note", () => {
    useNoteComposeStore.getState().resetComposeKeepQuotes();
    const state = useNoteComposeStore.getState();
    expect(state.body).toBe("");
    expect(state.selectedIndices).toEqual([1, 2, 3]);
    expect(state.anchorCueIndex).toBe(2);
  });
});
