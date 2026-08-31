import { describe, expect, it } from "vitest";

import { workspaceCwdFromMedia } from "./cwd";

describe("workspaceCwdFromMedia", () => {
  it("returns parent for windows and posix paths", () => {
    expect(workspaceCwdFromMedia("D:\\videos\\demo.mp4")).toBe("D:\\videos");
    expect(workspaceCwdFromMedia("/home/u/clips/a.mkv")).toBe("/home/u/clips");
  });

  it("returns undefined without a parent", () => {
    expect(workspaceCwdFromMedia(null)).toBeUndefined();
    expect(workspaceCwdFromMedia("video.mp4")).toBeUndefined();
  });
});
