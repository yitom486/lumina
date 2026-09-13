import { describe, expect, it } from "vitest";

import {
  defaultAgentProfiles,
  profilesHintFromStore,
} from "@lumina/chat-ui/defaultAgentProfiles";

describe("profilesHintFromStore", () => {
  it("falls back when persisted profiles are missing", () => {
    const hint = profilesHintFromStore("codex", undefined);
    expect(hint.activeProfileId).toBe("codex");
    expect(hint.profiles).toEqual(defaultAgentProfiles());
  });
});
