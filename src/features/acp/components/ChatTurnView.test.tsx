import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePlayerStore } from "@/features/player";

import type { ChatTurn } from "../types";
import { ChatShell, ChatColumn } from "./ChatShell";
import { ChatTurnView } from "./ChatTurnView";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

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
  vi.clearAllMocks();
  localStorage.clear();
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
  it("keeps assistant bubble full column width while streaming", async () => {
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
    expect(await screen.findByText("流式片段")).toBeInTheDocument();
    expect(container.textContent).toContain("▍");
  });

  it("uses the same full-width assistant bubble when done", async () => {
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
    expect(await screen.findByText("最终答案")).toBeInTheDocument();
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

describe("ChatTurnView save answer as note", () => {
  function renderDoneTurn(turn: ChatTurn) {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return render(
      <QueryClientProvider client={client}>
        <ChatTurnView turn={turn} />
      </QueryClientProvider>,
    );
  }

  beforeEach(() => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "notes_create")
        return Promise.resolve({
          id: "n1",
          mediaPath: "C:\\v\\a.mp4",
          positionMs: 192_000,
          body: "答案正文",
        });
      return Promise.resolve(null);
    });
    usePlayerStore.setState({
      currentFile: "C:\\v\\a.mp4",
      status: "Paused",
      currentTimeMs: 600_000,
    });
  });

  it("saves with the turn anchor after inline confirm", async () => {
    renderDoneTurn(
      makeTurn({
        id: "t8",
        userText: "解释这一段",
        answer: "答案正文",
        status: "done",
        showActivities: false,
        anchorMs: 192_000,
      }),
    );
    fireEvent.click(screen.getByText("存为批注"));
    // Confirm shows the turn anchor, not the live position.
    expect(await screen.findByText(/保存到 3:12 的批注？/)).toBeInTheDocument();
    fireEvent.click(screen.getByText("保存"));
    await waitFor(() => {
      expect(screen.getByText("已保存为批注")).toBeInTheDocument();
    });
    const create = vi
      .mocked(invoke)
      .mock.calls.find(([cmd]) => cmd === "notes_create");
    expect(create?.[1]).toMatchObject({
      input: expect.objectContaining({
        mediaPath: "C:\\v\\a.mp4",
        positionMs: 192_000,
        body: "答案正文",
        includeQuotes: false,
      }),
    });
  });

  it("surfaces backend failures without saving", async () => {
    vi.mocked(invoke).mockImplementation(() => {
      throw { code: "IoError", message: "笔记读写失败" };
    });
    renderDoneTurn(
      makeTurn({
        id: "t9",
        userText: "q",
        answer: "答案正文",
        status: "done",
        showActivities: false,
        anchorMs: 192_000,
      }),
    );
    fireEvent.click(screen.getByText("存为批注"));
    fireEvent.click(await screen.findByText("保存"));
    await waitFor(() => {
      expect(screen.getByText("笔记读写失败")).toBeInTheDocument();
    });
    expect(screen.queryByText("已保存为批注")).not.toBeInTheDocument();
  });

  it("hides the button while streaming or without an answer", () => {
    renderDoneTurn(
      makeTurn({ id: "t10", answer: "片段", status: "streaming" }),
    );
    expect(screen.queryByText("存为批注")).not.toBeInTheDocument();
  });
});