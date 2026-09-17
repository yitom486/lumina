import { describe, expect, it } from "vitest";

import {
  pickAcceptableFiles,
  toPromptImageInput,
} from "./pastedImages";

describe("pickAcceptableFiles", () => {
  it("accepts supported images within caps", () => {
    const { accepted, rejected } = pickAcceptableFiles(
      [
        { name: "a.png", type: "image/png", size: 1024 },
        { name: "b.jpg", type: "image/jpeg", size: 2048 },
      ],
      0,
    );
    expect(accepted).toHaveLength(2);
    expect(rejected).toEqual([]);
  });

  it("rejects unsupported kinds and oversized files with reasons", () => {
    const { accepted, rejected } = pickAcceptableFiles(
      [
        { name: "v.svg", type: "image/svg+xml", size: 100 },
        { name: "big.png", type: "image/png", size: 9 * 1024 * 1024 },
      ],
      0,
    );
    expect(accepted).toEqual([]);
    expect(rejected).toHaveLength(2);
    expect(rejected[0]).toContain("格式");
    expect(rejected[1]).toContain("8MB");
  });

  it("caps at four images including already attached ones", () => {
    const files = [1, 2, 3].map((n) => ({
      name: `${n}.png`,
      type: "image/png",
      size: 100,
    }));
    const { accepted, rejected } = pickAcceptableFiles(files, 3);
    expect(accepted).toHaveLength(1);
    expect(rejected.join("")).toContain("4 张");
  });
});

describe("toPromptImageInput", () => {
  it("splits data URLs into wire input", () => {
    expect(
      toPromptImageInput({
        id: "x",
        mimeType: "image/png",
        dataUrl: "data:image/png;base64,aGVsbG8=",
      }),
    ).toEqual({ mimeType: "image/png", data: "aGVsbG8=" });
    expect(
      toPromptImageInput({ id: "x", mimeType: "image/png", dataUrl: "nope" }),
    ).toBeNull();
  });
});
