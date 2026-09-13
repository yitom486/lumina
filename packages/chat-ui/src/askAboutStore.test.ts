import { beforeEach, describe, expect, it } from "vitest";

import { useAskAboutStore } from "./askAboutStore";

beforeEach(() => {
  useAskAboutStore.setState({ request: null });
});

describe("askAboutStore", () => {
  it("holds a pending shortcut-ask until consumed", () => {
    expect(useAskAboutStore.getState().request).toBeNull();
    useAskAboutStore.getState().askAbout(192_000, "解释这一段");
    const pending = useAskAboutStore.getState().request;
    expect(pending).toMatchObject({ anchorMs: 192_000, text: "解释这一段" });
    const consumed = useAskAboutStore.getState().consume();
    expect(consumed).toEqual(pending);
    expect(useAskAboutStore.getState().request).toBeNull();
    expect(useAskAboutStore.getState().consume()).toBeNull();
  });

  it("bumps nonce so repeat asks re-trigger consumers", () => {
    useAskAboutStore.getState().askAbout(1000, "a");
    const first = useAskAboutStore.getState().request?.nonce;
    useAskAboutStore.getState().askAbout(1000, "a");
    const second = useAskAboutStore.getState().request?.nonce;
    expect(second).toBeGreaterThan(first ?? 0);
  });

  it("builds Chinese presets", async () => {
    const { explainSegmentPreset, summarizeChapterPreset } =
      await import("./askAboutStore");
    expect(explainSegmentPreset("台词内容", "3:12")).toContain("解释这一段");
    expect(explainSegmentPreset("台词内容", "3:12")).toContain("3:12");
    expect(summarizeChapterPreset("开场", "0:00–2:00")).toContain("总结本章");
  });
});
