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

  it("ships a cursor profile with local-auth reuse and acp args", () => {
    const cursor = defaultAgentProfiles().find(
      (profile) => profile.id === "cursor",
    );
    expect(cursor).toMatchObject({
      kind: "Cursor",
      args: ["acp"],
      authPolicy: "cursor-local",
    });
    expect(cursor?.command.trim()).not.toBe("");
  });
});
