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
    expect(screen.getByText("处理中…")).toBeInTheDocument();
  });

  it("hides activity feed after minimal finish", () => {
    const turn: ChatTurn = {
      id: "t4",
      userText: "q",
      answer: "最终答案",
      status: "done",
      showActivities: false,
      activities: [],
    };
    const { container } = render(<ChatTurnView turn={turn} />);
    expect(container.querySelector(".border-border\\/60")).toBeNull();
    expect(container.textContent).toContain("最终答案");
  });
});
