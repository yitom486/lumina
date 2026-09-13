import { describe, expect, it } from "vitest";

import { mediaInfoKey, mediaToolStatusKey } from "./queries";

describe("media query keys", () => {
  it("builds the shared media-info key", () => {
    expect(mediaInfoKey("C:\\v\\a.mp4")).toEqual(["mediaInfo", "C:\\v\\a.mp4"]);
  });

  it("keeps a stable tool-status key", () => {
    expect(mediaToolStatusKey()).toEqual(["media-tool-status"]);
  });
});
