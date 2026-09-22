import { describe, expect, it } from "vitest";

import { isDirectResolverReady } from "./resolverSettings";

describe("isDirectResolverReady", () => {
  it("works with saved api key without live connection", () => {
    expect(
      isDirectResolverReady({
        modelId: "gpt-4o-mini",
        modelBaseUrl: "https://example.com/v1",
        modelApiKeySaved: true,
        pendingApiKey: "",
      }),
    ).toBe(true);
  });
});
