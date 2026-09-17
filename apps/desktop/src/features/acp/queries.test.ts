import { describe, expect, it } from "vitest";

import { acpQueryKeys } from "./queries";

describe("acpQueryKeys", () => {
  it("sessionList key is stable per scope", () => {
    expect(acpQueryKeys.sessionList("codex", "D:\\movie")).toEqual(
      acpQueryKeys.sessionList("codex", "D:\\movie"),
    );
  });

  it("sessionList key isolates scopes", () => {
    expect(acpQueryKeys.sessionList("codex", "D:\\a")).not.toEqual(
      acpQueryKeys.sessionList("codex", "D:\\b"),
    );
    expect(acpQueryKeys.sessionList("codex", "D:\\a")).not.toEqual(
      acpQueryKeys.sessionList("claude", "D:\\a"),
    );
    expect(acpQueryKeys.sessionList("codex", null)).toEqual(
      acpQueryKeys.sessionList("codex", null),
    );
  });
});
