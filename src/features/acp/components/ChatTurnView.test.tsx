import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { ChatTurn } from "../types";
import { ChatShell, ChatColumn } from "./ChatShell";
import { ChatTurnView } from "./ChatTurnView";

function makeTurn(partial: Partial<ChatTurn> & Pick<ChatTurn, "id">): ChatTurn {
  return {
    userText: "用户问题",
    answer: "",
    status: "streaming",
    activities: [],
    showActivities: true,
    ...partial,
  };
}

afterEach(() => {
  cleanup();
});

describe("ChatShell", () => {
  it("uses a unified max-width column", () => {
    const { container } = render(
      <ChatShell>
        <ChatColumn>content</ChatColumn>
      </ChatShell>,
    );
    const column = container.querySelector(".max-w-2xl");
    expect(column).toBeTruthy();
    expect(screen.getByText("content").parentElement).toHaveClass("w-full");
  });
});

describe("ChatTurnView", () => {
  it("keeps assistant bubble full column width while streaming", () => {
    const { container } = render(
      <ChatTurnView
        turn={makeTurn({
          id: "t1",
          answer: "流式片段",
          status: "streaming",
        })}
      />,
    );
    const assistant = container.querySelector(".bg-muted\\/40");
    expect(assistant).toHaveClass("w-full");
    expect(screen.getByText("流式片段")).toBeInTheDocument();
    expect(container.textContent).toContain("▍");
  });

  it("uses the same full-width assistant bubble when done", () => {
    const { container } = render(
      <ChatTurnView
        turn={makeTurn({
          id: "t2",
          answer: "最终答案",
          status: "done",
          showActivities: false,
        })}
      />,
    );
    const assistant = container.querySelector(".bg-muted\\/40");
    expect(assistant).toHaveClass("w-full");
    expect(screen.getByText("最终答案")).toBeInTheDocument();
    expect(container.textContent).not.toContain("▍");
  });

  it("shows tool activity feed during streaming", () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "t3",
          activities: [
            {
              id: "tool-1",
              kind: "tool",
              toolCallId: "1",
              title: "Read file",
              status: "running",
            },
          ],
          showActivities: true,
        })}
      />,
    );
    expect(screen.getByText("Read file")).toBeInTheDocument();
    expect(screen.getByText("工具执行中")).toBeInTheDocument();
  });

  it("shows failed tool detail in Chinese", () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "t5",
          activities: [
            {
              id: "tool-1",
              kind: "tool",
              toolCallId: "1",
              title: "读取播放上下文",
              status: "failed",
              text: "无法读取当前播放上下文",
            },
          ],
          showActivities: true,
        })}
      />,
    );
    expect(screen.getByText("失败")).toBeInTheDocument();
    expect(screen.getByText("无法读取当前播放上下文")).toBeInTheDocument();
  });

  it("offers expanding tool history after minimal finish", () => {
    const turn: ChatTurn = {
      id: "t4",
      userText: "q",
      answer: "最终答案",
      status: "done",
      showActivities: false,
      activities: [
        {
          id: "tool-1",
          kind: "tool",
          toolCallId: "1",
          title: "读取库信息",
          status: "failed",
          text: "当前媒体未关联媒体库目录",
        },
      ],
    };
    render(<ChatTurnView turn={turn} />);
    expect(screen.getByText("查看工具执行（1）")).toBeInTheDocument();
    expect(screen.queryByText("读取库信息")).not.toBeInTheDocument();
  });

  it("renders annotation proposal card below assistant bubble", () => {
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <ChatTurnView
          turn={makeTurn({
            id: "t6",
            answer: "这是分析",
            status: "done",
            annotationProposal: {
              proposalId: "proposal-1",
              mediaPath: "D:\\show.mkv",
              positionMs: 90_000,
              body: "批注正文",
              includeQuotes: true,
              quotes: [],
              previewMarkdown: "### 1:30\n\n批注正文",
              createdAtMs: 1,
            },
          })}
          annotationWorkspace="D:\\"
          onDismissAnnotation={() => undefined}
          onSaveAnnotation={() => undefined}
        />
      </QueryClientProvider>,
    );
    expect(screen.getByText("Agent 提议批注")).toBeInTheDocument();
    expect(screen.getByText("确认保存")).toBeInTheDocument();
    expect(screen.getByDisplayValue("批注正文")).toBeInTheDocument();
  });

  it("shows saved status after annotation is confirmed", () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "t7",
          answer: "已生成批注提议，待确认。",
          status: "done",
          annotationProposalSaved: true,
        })}
      />,
    );
    expect(screen.getByText("批注已写入笔记库")).toBeInTheDocument();
    expect(screen.queryByText("确认保存")).not.toBeInTheDocument();
  });
});