import { describe, expect, it } from "vitest";

import { noteFrameKey, notesKey } from "@lumina/query-keys";

describe("notes query keys", () => {
  it("builds the shared notes key", () => {
    expect(notesKey("C:\\v\\a.mp4")).toEqual(["notes", "C:\\v\\a.mp4"]);
  });

  it("builds the frame key per note", () => {
    expect(noteFrameKey("n1")).toEqual(["note-frame", "n1"]);
  });
});
