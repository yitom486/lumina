import { describe, expect, it } from "vitest";

import { formatRenderError } from "./formatRenderError";

describe("formatRenderError", () => {
  it("maps infinite loop to actionable Chinese copy", () => {
    const copy = formatRenderError(
      new Error("Maximum update depth exceeded"),
      "对话",
    );
    expect(copy.message).toMatch(/无法完成加载/);
    expect(copy.hint).not.toMatch(/开发者工具/);
  });
});
