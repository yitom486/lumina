import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import { applyAcpEventToTurn, createTurn } from "@lumina/chat-ui/chatTurns";

import type { LoadedTranscriptEvent } from "../api";
import { mapLoadedTranscript } from "../conversationTranscript";
import { ChatTurnView } from "./ChatTurnView";

afterEach(cleanup);

// 见 ChatTurnView.test.tsx：预热 React.lazy 的 ChatMarkdown 分包。
beforeAll(async () => {
  await import("./ChatMarkdown");
});

describe("conversation presentation integration", () => {
  it("projects an original-shaped restored transcript into a safe rich card", async () => {
    const originalReplay: LoadedTranscriptEvent[] = [
      {
        role: "user",
        text: [
          "台词上下文窗口建议：当前播放点前后各 30 秒；读取当前台词时优先使用该范围。",
          "请梳理目前剧情。",
        ].join("\n"),
      },
      { role: "agent", text: "Designing JSON schema for plot summary" },
      { role: "tool", text: "lumina_get_transcript_window { path: 'D:/private.mkv' }" },
      { role: "agent", text: "Confirming timestamp boundaries for summary" },
      {
        role: "agent",
        text: JSON.stringify({
          version: "plot_summary.v1",
          scope: { label: "截至当前播放位置" },
          summary: "两位主角在争执后重新确认了彼此的心意。",
          evidence: [
            { source: "transcript_window", text: "字幕提到两人决定一起回去。" },
          ],
        }),
      },
    ];

    const [turn] = mapLoadedTranscript(originalReplay);
    expect(turn?.shortcutTaskId).toBe("plot_summary");
    expect(turn?.activities).toEqual([]);

    render(<ChatTurnView turn={turn!} presentationMode="release" />);

    expect(
      await screen.findByRole("heading", { name: "剧情梳理" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("两位主角在争执后重新确认了彼此的心意。"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/plot_summary\.v1/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Designing JSON schema/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Confirming timestamp/)).not.toBeInTheDocument();
    expect(screen.queryByText(/lumina_get_transcript_window/)).not.toBeInTheDocument();
    expect(screen.queryByText(/台词上下文窗口建议/)).not.toBeInTheDocument();
  });

  it("keeps development diagnostics separate and release status business-facing", () => {
    const sequence = { n: 0 };
    let turn = createTurn(sequence, "这段在讲什么？");
    turn = applyAcpEventToTurn(
      turn,
      { type: "agentThought", text: "Planning with C:\\private\\prompt.md" },
      "verbose",
    );
    turn = applyAcpEventToTurn(
      turn,
      { type: "plan", text: "Read transcript then return JSON" },
      "verbose",
    );
    turn = applyAcpEventToTurn(
      turn,
      {
        type: "toolCall",
        toolCallId: "call-1",
        title: "lumina_get_transcript_window",
        status: "running",
        detail: '{"path":"C:\\\\private\\\\video.mkv","stderr":"no"}',
      },
      "verbose",
    );

    const development = render(
      <ChatTurnView turn={turn} presentationMode="development" />,
    );
    expect(screen.getByText("工具：lumina_get_transcript_window")).toBeInTheDocument();
    expect(screen.getByText("思路整理")).toBeInTheDocument();
    expect(screen.queryByText(/private\\\\video|prompt\.md|stderr/)).not.toBeInTheDocument();
    development.unmount();

    const release = render(<ChatTurnView turn={turn} presentationMode="release" />);
    expect(screen.getByText("正在读取该段台词")).toBeInTheDocument();
    expect(screen.queryByText(/lumina_get_transcript_window/)).not.toBeInTheDocument();
    expect(screen.queryByText(/private\\\\video|prompt\.md|stderr/)).not.toBeInTheDocument();
    release.unmount();

    const done = applyAcpEventToTurn(
      turn,
      { type: "finished", text: "最终回答只保留这句话。" },
      "verbose",
    );
    const completedRelease = render(
      <ChatTurnView turn={done} presentationMode="release" />,
    );
    expect(screen.getByText("最终回答只保留这句话。")).toBeInTheDocument();
    expect(screen.queryByText("正在读取该段台词")).not.toBeInTheDocument();
    expect(screen.queryByText(/lumina_get_transcript_window/)).not.toBeInTheDocument();
    completedRelease.unmount();

    render(<ChatTurnView turn={done} presentationMode="development" />);
    expect(screen.getByText("最终回答只保留这句话。")).toBeInTheDocument();
    expect(screen.getByText(/查看本轮调试记录/)).toBeInTheDocument();
  });
});
