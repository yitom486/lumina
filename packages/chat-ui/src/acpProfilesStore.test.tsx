import { beforeEach, describe, expect, it } from "vitest";

import { useAcpProfilesStore } from "./acpProfilesStore";

beforeEach(() => {
  localStorage.clear();
  useAcpProfilesStore.setState({ activeProfileId: "codex" });
});

describe("switchActiveProfileId (switch world entry)", () => {
  it("flips the active key to the target profile", () => {
    useAcpProfilesStore.getState().switchActiveProfileId("claude");
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("claude");
  });

  it("is a no-op for the same id and for blank ids", () => {
    useAcpProfilesStore.getState().switchActiveProfileId("codex");
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("codex");

    useAcpProfilesStore.getState().switchActiveProfileId("  ");
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("codex");
  });

  it("keeps the profile list intact (switching never edits the registry)", () => {
    const before = useAcpProfilesStore.getState().profiles.map((p) => p.id);
    useAcpProfilesStore.getState().switchActiveProfileId("cursor");
    const after = useAcpProfilesStore.getState().profiles.map((p) => p.id);
    expect(after).toEqual(before);
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("cursor");
  });

  it("leaves setActiveProfileId as the bare setter", () => {
    // Bare setter flips blindly (legacy direct writes); the guarded entry above
    // is what UI switches must use.
    useAcpProfilesStore.getState().setActiveProfileId("cursor");
    expect(useAcpProfilesStore.getState().activeProfileId).toBe("cursor");
  });
});
