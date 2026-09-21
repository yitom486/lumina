import { beforeEach, describe, expect, it } from "vitest";

import type { ChapterProgressEvent } from "./api";
import { useChapterProgressStore } from "./progressStore";

function event(overrides: Partial<ChapterProgressEvent> = {}): ChapterProgressEvent {
  return {
    taskKey: "chapter-segmentation:test",
    taskId: 7,
    attemptId: 1,
    phase: "agent_running",
    message: "正在执行章节 Agent",
    attemptCount: 1,
    maxAttempts: 3,
    sequence: 1,
    committed: false,
    updatedAtMs: 100,
    ...overrides,
  };
}

describe("chapter progress projection", () => {
  beforeEach(() => {
    useChapterProgressStore.setState({ byTaskKey: {} });
  });

  it("keeps the newest event while allowing same-timestamp sequence updates", () => {
    const store = useChapterProgressStore.getState();
    store.upsert(event({ sequence: 2, phase: "building_outline" }));
    store.upsert(event({ sequence: 1, phase: "agent_running" }));
    store.upsert(event({ sequence: 3, phase: "capturing_evidence" }));

    expect(
      useChapterProgressStore.getState().byTaskKey["chapter-segmentation:test"],
    ).toMatchObject({ phase: "capturing_evidence", sequence: 3 });
  });

  it("does not replace a newer committed projection with a late event", () => {
    const store = useChapterProgressStore.getState();
    store.upsert(event({ phase: "succeeded", committed: true, updatedAtMs: 200 }));
    store.upsert(event({ phase: "agent_running", updatedAtMs: 199 }));

    expect(
      useChapterProgressStore.getState().byTaskKey["chapter-segmentation:test"],
    ).toMatchObject({ phase: "succeeded", committed: true, updatedAtMs: 200 });
  });

  it("accepts the first event of a new retry attempt even if its clock value ties", () => {
    const store = useChapterProgressStore.getState();
    store.upsert(event({ phase: "retrying", attemptCount: 1, sequence: 9 }));
    store.upsert(
      event({ phase: "running", attemptCount: 2, sequence: 1, updatedAtMs: 100 }),
    );

    expect(
      useChapterProgressStore.getState().byTaskKey["chapter-segmentation:test"],
    ).toMatchObject({ phase: "running", attemptCount: 2, sequence: 1 });
  });
});
