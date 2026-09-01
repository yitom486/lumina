import { describe, expect, it } from "vitest";

import {
  isToolFailed,
  mergeToolDetail,
  toolFailureHint,
  toolStatusLabel,
} from "./toolStatus";

describe("toolStatus", () => {
  it("maps failed status to Chinese label", () => {
    expect(toolStatusLabel("failed")).toBe("失败");
    expect(isToolFailed("failed")).toBe(true);
  });

  it("prefers detail for failure hint", () => {
    expect(toolFailureHint("failed", "无法读取当前播放上下文")).toBe(
      "无法读取当前播放上下文",
    );
    expect(
      toolFailureHint(
        "failed",
        '{"result":{"content":[{"type":"text","text":"当前媒体未关联媒体库目录"}]}}',
      ),
    ).toBe("当前媒体未关联媒体库目录");
    expect(toolFailureHint("failed")).toBe(
      "工具执行未成功，Agent 将尝试其他方式继续",
    );
  });

  it("merges appended tool detail", () => {
    expect(mergeToolDetail("第一行", "第二行", true)).toBe("第一行\n第二行");
    expect(mergeToolDetail("旧内容", "新内容", false)).toBe("新内容");
  });
});
