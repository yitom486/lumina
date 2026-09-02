import { afterEach, describe, expect, it } from "vitest";

import { focusComposerTextarea } from "./composerFocus";

describe("focusComposerTextarea", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("focuses enabled textarea and places caret at end", () => {
    const textarea = document.createElement("textarea");
    textarea.value = "hello";
    document.body.appendChild(textarea);

    focusComposerTextarea(textarea);

    expect(document.activeElement).toBe(textarea);
    expect(textarea.selectionStart).toBe(5);
    expect(textarea.selectionEnd).toBe(5);
  });

  it("skips disabled textarea", () => {
    const textarea = document.createElement("textarea");
    textarea.disabled = true;
    document.body.appendChild(textarea);

    focusComposerTextarea(textarea);

    expect(document.activeElement).not.toBe(textarea);
  });
});
