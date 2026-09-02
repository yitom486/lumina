import { describe, expect, it } from "vitest";

import {
  bargeInPrompt,
  createQueuedPrompt,
  dequeuePrompt,
  enqueuePrompt,
  previewQueuedText,
  removeQueuedPrompt,
} from "./promptQueue";

describe("promptQueue", () => {
  it("enqueues FIFO and dequeues head first", () => {
    const a = createQueuedPrompt("第一句", 1000, "a");
    const b = createQueuedPrompt("第二句", 2000, "b");
    let queue = enqueuePrompt([], a);
    queue = enqueuePrompt(queue, b);
    expect(queue.map((item) => item.id)).toEqual(["a", "b"]);

    const first = dequeuePrompt(queue);
    expect(first.next?.text).toBe("第一句");
    expect(first.rest.map((item) => item.id)).toEqual(["b"]);
  });

  it("barge-in replaces the whole queue with one prompt", () => {
    const a = createQueuedPrompt("排队中", 1, "a");
    const b = createQueuedPrompt("还在排", 2, "b");
    const queue = enqueuePrompt(enqueuePrompt([], a), b);
    const next = bargeInPrompt(queue, createQueuedPrompt("插队优先", 3, "c"));
    expect(next).toHaveLength(1);
    expect(next[0]?.text).toBe("插队优先");
  });

  it("remove and empty dequeue are safe", () => {
    const a = createQueuedPrompt("x", 1, "a");
    const queue = removeQueuedPrompt([a], "a");
    expect(queue).toEqual([]);
    expect(dequeuePrompt([]).next).toBeNull();
  });

  it("previews long queued text", () => {
    expect(previewQueuedText("短")).toBe("短");
    expect(previewQueuedText("这是一段很长很长很长很长很长的排队文本")).toContain(
      "…",
    );
  });
});
