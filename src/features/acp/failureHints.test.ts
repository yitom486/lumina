import { describe, expect, it } from "vitest";

import { hintForAcpFailure } from "./failureHints";

describe("hintForAcpFailure", () => {
  it("returns actionable Chinese hints without tool names", () => {
    const hint = hintForAcpFailure("ProtocolError");
    expect(hint).toMatch(/新建对话/);
    expect(hint).not.toMatch(/JSON|stderr/i);
    const auth = hintForAcpFailure("NotConfigured");
    expect(auth).toMatch(/codex login|登录/);
  });
});
