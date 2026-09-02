import { describe, expect, it } from "vitest";

import {
  composeAssistantAnswer,
  dedupeRepeatedBlocks,
  stripEnglishTranslationBlocks,
} from "./assistantAnswer";

describe("stripEnglishTranslationBlocks", () => {
  it("removes Natural English lines and inline backticks", () => {
    const raw =
      "Natural English: Is the tool available?\n\n我先检查工具列表。\n`Natural English: Is the tool available?`\n\n目前还不行。";
    expect(stripEnglishTranslationBlocks(raw)).toBe(
      "我先检查工具列表。\n\n目前还不行。",
    );
  });

  it("removes English: prefix variant", () => {
    expect(stripEnglishTranslationBlocks("**English:** Hello\n\n你好")).toBe(
      "你好",
    );
  });

  it("removes multiline Natural English blocks", () => {
    const raw =
      "Natural English:\n\nIs the tool available?\n\nNatural English:\n\nIs the tool available?\n\n目前还不行。";
    expect(stripEnglishTranslationBlocks(raw)).toBe("目前还不行。");
  });

  it("removes both English and Natural English labels in one reply", () => {
    const raw =
      "English: Check tools\n\nNatural English: Check tools\n\n我先检查 MCP 工具。";
    expect(stripEnglishTranslationBlocks(raw)).toBe("我先检查 MCP 工具。");
  });
});

describe("dedupeRepeatedBlocks", () => {
  it("drops duplicate paragraphs", () => {
    expect(
      dedupeRepeatedBlocks("段落一\n\n段落二\n\n段落一\n\n段落三"),
    ).toBe("段落一\n\n段落二\n\n段落三");
  });
});

describe("composeAssistantAnswer", () => {
  it("merges stream cleanup with thought overlap removal", () => {
    const raw =
      "Natural English: Check tools\n\n我先检查 MCP 工具。\n我先检查 MCP 工具。\n结论：还不行。";
    const result = composeAssistantAnswer(raw, [
      { id: "t1", kind: "thought", text: "我先检查 MCP 工具。" },
    ]);
    expect(result).toBe("我先检查 MCP 工具。\n结论：还不行。");
    expect(result).not.toMatch(/Natural English/i);
  });
});
