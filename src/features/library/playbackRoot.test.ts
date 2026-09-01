import { describe, expect, it } from "vitest";

import {
  libraryRootFromPlaybackPath,
  shouldSyncLibraryRoots,
} from "./playbackRoot";

describe("libraryRootFromPlaybackPath", () => {
  it("returns parent directory on Windows paths", () => {
    expect(
      libraryRootFromPlaybackPath(
        "D:\\Shows\\We Are All Trying Here\\S01E01.mkv",
      ),
    ).toBe("D:\\Shows\\We Are All Trying Here");
  });

  it("returns null when file has no parent", () => {
    expect(libraryRootFromPlaybackPath("movie.mkv")).toBeNull();
  });
});

describe("shouldSyncLibraryRoots", () => {
  it("syncs when follow mode is on and directory changed", () => {
    expect(
      shouldSyncLibraryRoots(
        true,
        "D:\\Shows\\Series\\ep1.mkv",
        ["D:\\Other"],
      ),
    ).toEqual(["D:\\Shows\\Series"]);
  });

  it("skips when manual mode is off", () => {
    expect(
      shouldSyncLibraryRoots(
        false,
        "D:\\Shows\\Series\\ep1.mkv",
        ["D:\\Other"],
      ),
    ).toBeNull();
  });

  it("skips when roots already match", () => {
    expect(
      shouldSyncLibraryRoots(
        true,
        "D:\\Shows\\Series\\ep1.mkv",
        ["D:\\Shows\\Series"],
      ),
    ).toBeNull();
  });
});
