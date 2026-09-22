import { describe, expect, it } from "vitest";

import {
  acceptsProgressEvent,
  claimProgressOwner,
  sealProgressOwner,
} from "./progressOwner";

describe("progressOwner", () => {
  it("lets only the unsealed owner write or clear the progress line", () => {
    let owner = claimProgressOwner("turn-1");
    expect(acceptsProgressEvent(owner, "turn-1")).toBe(true);
    expect(acceptsProgressEvent(owner, "turn-0")).toBe(false);
    expect(acceptsProgressEvent(null, "turn-1")).toBe(false);

    owner = sealProgressOwner(owner, "turn-1");
    expect(acceptsProgressEvent(owner, "turn-1")).toBe(false);
  });

  it("ignores seals from another run and keeps the newer owner", () => {
    const run1 = claimProgressOwner("turn-1");
    const run2 = claimProgressOwner("turn-2");

    // 上一轮的 finished 落到新主人头上：不许清行。
    expect(acceptsProgressEvent(run2, "turn-1")).toBe(false);
    expect(sealProgressOwner(run2, "turn-1")).toBe(run2);

    // 上报的串台场景：finished 之后又来一条迟到 progress。
    expect(acceptsProgressEvent(sealProgressOwner(run1, "turn-1"), "turn-1")).toBe(
      false,
    );
  });
});
