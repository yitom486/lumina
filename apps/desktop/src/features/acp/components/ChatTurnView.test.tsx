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
import { ChatShell, ChatColumn } from "@lumina/chat-ui/components/ChatShell";
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
  it("renders a complete structured answer through the rich block renderer", async () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "rich-1",
          userText: "整理这段内容",
          answer: JSON.stringify({
            blocks: [
              {
                kind: "watch-feed-card",
                title: "当前观察",
                summary: "结构化输出",
                spoilerLevel: "current",
                actions: [],
              },
            ],
          }),
          status: "done",
          showActivities: false,
        })}
      />,
    );

    expect(await screen.findByText("当前观察")).toBeInTheDocument();
    expect(screen.getByText("结构化输出")).toBeInTheDocument();
    expect(document.querySelector("[data-rich-blocks]")).toBeInTheDocument();
  });

  it("renders the raw plot summary shortcut contract as a rich card", async () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "plot-summary-1",
          userText: "剧情梳理",
          shortcutTaskId: "plot_summary",
          answer: JSON.stringify({
            version: "plot_summary.v1",
            scope: { label: "当前观看范围" },
            summary: "主角在车站重新确认了调查方向。",
            evidence: ["字幕：我们必须回到车站。"],
          }),
          status: "done",
          showActivities: false,
        })}
      />,
    );

    expect(
      await screen.findByRole("heading", { name: "剧情梳理" }),
    ).toBeInTheDocument();
    expect(screen.getByText("主角在车站重新确认了调查方向。")).toBeInTheDocument();
    expect(screen.getByText("字幕：我们必须回到车站。")).toBeInTheDocument();
    expect(screen.queryByText(/plot_summary\.v1/)).not.toBeInTheDocument();
    expect(document.querySelector("[data-rich-blocks]")).toBeInTheDocument();
  });

  it("renders another quick task contract without falling back to raw JSON", async () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "chapter-outlook-1",
          userText: "后续看点",
          shortcutTaskId: "chapter_outlook",
          answer: JSON.stringify({
            version: "chapter_outlook.v1",
            items: [{ title: "留意车站里的反应" }],
          }),
          status: "done",
          showActivities: false,
        })}
      />,
    );

    expect(
      await screen.findByRole("heading", { name: "后续看点" }),
    ).toBeInTheDocument();
    expect(screen.getByText("留意车站里的反应")).toBeInTheDocument();
    expect(screen.queryByText(/chapter_outlook\.v1/)).not.toBeInTheDocument();
  });

  it("uses a readable fallback when a shortcut contract is malformed", async () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "plot-summary-bad",
          userText: "剧情梳理",
          shortcutTaskId: "plot_summary",
          answer: '{"version":"plot_summary.v1"}',
          status: "done",
          showActivities: false,
        })}
      />,
    );

    expect(
      await screen.findByText("剧情梳理结果暂时无法展示，请稍后重试。"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/plot_summary\.v1/)).not.toBeInTheDocument();
  });

  it("forwards rich card actions when an assistant action callback is provided", async () => {
    const onAssistantAction = vi.fn();
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "rich-action-1",
          answer: JSON.stringify({
            blocks: [
              {
                kind: "action-chip",
                label: "跳转到这里",
                action: { type: "seek", anchor: { startMs: 42_000 } },
              },
            ],
          }),
          status: "done",
          showActivities: false,
        })}
        onAssistantAction={onAssistantAction}
      />,
    );

    const action = await screen.findByRole("button", { name: "跳转到这里" });
    expect(action).not.toBeDisabled();
    fireEvent.click(action);
    expect(onAssistantAction).toHaveBeenCalledWith({
      type: "seek",
      anchor: { startMs: 42_000 },
    });
  });

  it("keeps rich card actions disabled without a callback", async () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "rich-action-2",
          answer: JSON.stringify({
            blocks: [
              {
                kind: "action-chip",
                label: "安全操作",
                action: { type: "ask", anchor: { startMs: 1_000 }, prompt: "解释" },
              },
            ],
          }),
          status: "done",
          showActivities: false,
        })}
      />,
    );

    expect(await screen.findByRole("button", { name: "安全操作" })).toBeDisabled();
  });

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

  it("shows a safe development trace during streaming", () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "t3",
          activities: [
            {
              id: "tool-1",
              kind: "tool",
              toolCallId: "1",
              title: "lumina_get_transcript_window",
              status: "running",
              text: '{"path":"C:\\\\private\\\\video.mkv"}',
            },
          ],
          showActivities: true,
        })}
      />,
    );
    expect(screen.getByText("工具：lumina_get_transcript_window")).toBeInTheDocument();
    expect(screen.queryByText(/private\\\\video/)).not.toBeInTheDocument();
    expect(screen.getByText("工具执行中")).toBeInTheDocument();
  });

  it("does not show raw failed tool detail in the debug trace", () => {
    render(
      <ChatTurnView
        turn={makeTurn({
          id: "t5",
          activities: [
            {
              id: "tool-1",
              kind: "tool",
              toolCallId: "1",
              title: "lumina_get_playback_context",
              status: "failed",
              text: "stderr: C:\\private\\tool.exe failed",
            },
          ],
          showActivities: true,
        })}
      />,
    );
    expect(screen.getByText("失败")).toBeInTheDocument();
    expect(screen.getByText("执行未完成")).toBeInTheDocument();
    expect(screen.queryByText(/private\\tool/)).not.toBeInTheDocument();
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
    expect(screen.getByText("查看本轮调试记录（1 个工具）")).toBeInTheDocument();
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
