import { beforeEach, describe, expect, it } from "vitest";

import { followMode, useFollowStore } from "./followStore";

beforeEach(() => {
  useFollowStore.setState({ followEnabled: true, browsing: false });
});

describe("followStore", () => {
  it("defaults to following", () => {
    const state = useFollowStore.getState();
    expect(state.followEnabled).toBe(true);
    expect(state.browsing).toBe(false);
    expect(followMode(state.followEnabled, state.browsing)).toBe("following");
  });

  it("manual browsing pauses follow without touching the preference", () => {
    useFollowStore.getState().setBrowsing(true);
    const state = useFollowStore.getState();
    expect(state.followEnabled).toBe(true);
    expect(followMode(state.followEnabled, state.browsing)).toBe("browsing");
  });

  it("resume and media switch clear browsing", () => {
    useFollowStore.getState().setBrowsing(true);
    useFollowStore.getState().resume();
    expect(useFollowStore.getState().browsing).toBe(false);
    useFollowStore.getState().setBrowsing(true);
    useFollowStore.getState().resetForMedia();
    expect(useFollowStore.getState().browsing).toBe(false);
  });

  it("disabling follow reports off; re-enabling resumes", () => {
    useFollowStore.getState().setBrowsing(true);
    useFollowStore.getState().setFollowEnabled(false);
    expect(followMode(false, true)).toBe("off");
    useFollowStore.getState().setFollowEnabled(true);
    const state = useFollowStore.getState();
    expect(state.browsing).toBe(false);
    expect(followMode(state.followEnabled, state.browsing)).toBe("following");
  });
});
