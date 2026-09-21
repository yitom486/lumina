import { describe, expect, it } from "vitest";

import { presentChatActivities } from "./presentationPolicy";

describe("presentChatActivities", () => {
  const rawActivities = [
    {
      id: "tool-1",
      kind: "tool" as const,
      title: "lumina_get_transcript_window",
      toolCallId: "call-1",
      status: "running",
      text: '{"path":"C:\\\\private\\\\episode.mkv","stderr":"raw"}',
    },
    {
      id: "thought-1",
      kind: "thought" as const,
      text: "Designing JSON schema with a private system prompt",
    },
  ];

  it("keeps only an allowlisted tool identifier and safe labels in development", () => {
    const presented = presentChatActivities(rawActivities, "development");
    expect(presented[0]).toMatchObject({
      title: "工具：lumina_get_transcript_window",
      text: "正在执行",
    });
    expect(JSON.stringify(presented)).not.toMatch(/private|stderr|JSON schema/);
    expect(presented[1]).toMatchObject({
      title: "思路整理",
      text: "正在整理回答思路",
    });
  });

  it("uses business aliases and no implementation identifier in release", () => {
    const presented = presentChatActivities(rawActivities, "release");
    expect(presented[0]).toMatchObject({ title: "正在读取该段台词" });
    expect(JSON.stringify(presented)).not.toMatch(
      /lumina_get_transcript_window|private|stderr|JSON schema/,
    );
  });
});
